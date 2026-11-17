#!/usr/bin/env python3
r"""
WAC (Win At Chess) test runner for KnightByte — and any UCI engine.

Reads an EPD file (one tactical position per line, expected best move in the
`bm` opcode), runs the engine with a fixed time per position and reports how
many positions were solved.

Configurable:
    --epd        path to the EPD file          (default: wacnew.epd next to this script)
    --engine     path to the UCI engine        (default: $KNIGHTBYTE_ENGINE or target/release/knightbyte)
    --positions  how many positions to test    (default: all)
    --workers    number of parallel engines    (default: CPU count)
    --hash       hash size in MB per engine    (default: 64)
    --movetime   time per position in ms       (default: 1000)

Examples:

python perf-test/wac_test.py \
  --engine target/release/knightbyte \
  --positions 300 --workers 1 --hash 128 --movetime 1000

    python wac_test.py
    python wac_test.py --engine /path/to/baseline/engine --positions 300 --workers 1 --hash 128 --movetime 1000
"""

import argparse
import os

# Ensure a UTF-8 locale is set: some .NET engines on macOS fail at startup
# with 'System.ArgumentException: 1 is not a supported code page' when
# LANG/LC_ALL are unset. Harmless for native engines.
if not os.environ.get("LANG"):
    os.environ["LANG"] = "en_US.UTF-8"
if not os.environ.get("LC_ALL"):
    os.environ["LC_ALL"] = "en_US.UTF-8"

import shlex
import shutil
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor, as_completed

import chess
import chess.engine

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))


def parse_args():
    p = argparse.ArgumentParser(description="WAC test runner (EPD + UCI engine)")
    p.add_argument("--epd", default=os.path.join(SCRIPT_DIR, "wacnew.epd"),
                   help="EPD file with bm opcodes (default: wacnew.epd)")
    p.add_argument("--engine", default=(
                       os.environ.get("KNIGHTBYTE_ENGINE", "").strip()
                       or os.environ.get("KNIGHTBYTE_BASELINE_ENGINE", "").strip()
                       or os.path.join(SCRIPT_DIR, "..", "target", "release",
                                       "knightbyte" + (".exe" if os.name == "nt" else ""))),
                   help="UCI engine executable (default: $KNIGHTBYTE_ENGINE or "
                        "target/release/knightbyte)")
    p.add_argument("--positions", type=int, default=0,
                   help="number of positions to test (0 = all)")
    p.add_argument("--workers", type=int, default=os.cpu_count() or 4,
                   help="number of parallel engine instances")
    p.add_argument("--hash", type=int, default=64,
                   help="hash size in MB per engine instance")
    p.add_argument("--movetime", type=int, default=1000,
                   help="time per position in milliseconds")
    p.add_argument("--setoption", action="append", default=[], metavar="NAME=VALUE",
                   help="extra UCI setoption to send to each engine (repeatable), "
                        "e.g. --setoption \"Use Improving Flag=false\"")
    return p.parse_args()


def load_positions(epd_path, limit):
    """Parses the EPD file. Returns a list of dicts: id, fen, solutions (UCI list)."""
    tasks = []
    skipped = 0
    with open(epd_path, encoding="utf-8") as f:
        for line_no, raw in enumerate(f, 1):
            line = raw.strip()
            if not line:
                continue

            # EPD layout: <4 board fields> <opcode1> [value1]; <opcode2> [value2]; ...
            # NOTE: the FIRST opcode sits before the first semicolon!
            segments = line.split(";")
            head = segments[0].split()

            if len(head) < 5:
                skipped += 1
                continue
            fen = " ".join(head[:4]) + " 0 1"

            opcodes = {head[4]: " ".join(head[5:]).strip().strip('"')}
            for chunk in segments[1:]:
                chunk = chunk.strip()
                if not chunk:
                    continue
                key, _, value = chunk.partition(" ")
                opcodes[key.strip()] = value.strip().strip('"')

            bm = opcodes.get("bm", "")
            pid = opcodes.get("id", f"line{line_no}")

            board = chess.Board(fen)
            solutions = []
            for san in bm.split():
                san_clean = san.rstrip("+#!?")
                try:
                    solutions.append(board.parse_san(san_clean).uci())
                except ValueError:
                    print(f"  WARNING: {pid}: cannot parse bm '{san}' — position skipped")
                    skipped += 1
                    solutions = []
                    break

            if solutions:
                tasks.append({"id": pid, "fen": fen, "solutions": solutions})
            if limit and len(tasks) >= limit:
                break

    if skipped:
        print(f"  {skipped} line(s) skipped (unparseable)")
    return tasks


class EnginePool:
    """One UCI engine process per worker thread (thread-local)."""

    def __init__(self, engine_cmd, hash_mb, extra_options=None):
        self.engine_cmd = engine_cmd  # list of argv tokens
        self.hash_mb = hash_mb
        # Extra UCI options (dict NAME->VALUE) applied to every engine instance.
        # Used to flip the hidden Wave-1 search toggles for A/B isolation runs.
        self.extra_options = dict(extra_options or {})
        self.local = threading.local()
        self.lock = threading.Lock()
        self.engines = []

    def get(self):
        if not hasattr(self.local, "engine"):
            try:
                engine = chess.engine.SimpleEngine.popen_uci(self.engine_cmd, debug=False)
                # The Wave-1 search toggles are HIDDEN options: the engine does not
                # announce them in the `uci` handshake, so python-chess never learns
                # about them and engine.configure() would raise KeyError. Register a
                # synthetic check-type Option for each one BEFORE configuring, so the
                # library will accept the value and send the correct `setoption` line.
                for name in self.extra_options:
                    if name not in engine.options:
                        engine.protocol.options[name] = chess.engine.Option(
                            name, "check", "true", None, None, None)
                config = {"Hash": self.hash_mb, "Threads": 1}
                config.update(self.extra_options)
                engine.configure(config)
            except Exception as ex:
                raise RuntimeError(
                    f"worker could not start engine {self.engine_cmd}: {ex}") from ex
            self.local.engine = engine
            with self.lock:
                self.engines.append(engine)
        return self.local.engine

    def close_all(self):
        for engine in self.engines:
            try:
                engine.quit()
            except Exception:
                pass


def solve_task(pool, task, movetime_s):
    engine = pool.get()
    board = chess.Board(task["fen"])
    try:
        result = engine.play(board, chess.engine.Limit(time=movetime_s), game=object())
        got = result.move.uci() if result.move else "(none)"
    except (chess.engine.EngineError, chess.engine.EngineTerminatedError) as ex:
        return task, f"ENGINE-ERROR: {ex}"
    return task, got


def split_command(cmd):
    """Split a command line into argv, using the RULES OF THIS PLATFORM.

    `shlex.split()` defaults to posix=True, where a backslash is an ESCAPE
    character. On Windows that turns the natural

        target\\release\\engine.exe

    into `targetreleaseengine.exe`, so the tool reports "engine executable not
    found" for a file that is sitting right there. The same helper is used in
    perf_test.py and ebf_compare.py.

    On Windows use the native CommandLineToArgvW rules (posix=False), then drop
    the quotes that mode keeps, so a quoted path containing spaces stays one
    argument.
    """
    if os.name == "nt":
        parts = shlex.split(cmd, posix=False)
        return [p[1:-1] if len(p) >= 2 and p[0] == p[-1] == '"' else p for p in parts]
    return shlex.split(cmd)


def main():
    args = parse_args()

    # --engine accepts a path OR a full command line (e.g. for jar engines):
    #   --engine target/release/knightbyte
    #   --engine "java -jar /path/to/reference-engine.jar"
    engine_cmd = split_command(args.engine)
    if not engine_cmd or not (os.path.sep in engine_cmd[0] and os.path.exists(engine_cmd[0])
                              or shutil.which(engine_cmd[0])):
        print(f"ERROR: engine executable not found: {engine_cmd[0]}")
        sys.exit(1)
    if not os.path.exists(args.epd):
        print(f"ERROR: EPD file not found: {args.epd}")
        sys.exit(1)

    # Startup smoke test: verify the engine answers the UCI handshake BEFORE
    # launching the full run. Catches bad paths, wrong quotes (e.g. typographic
    # “ ” instead of ASCII ") or missing jar files with a clear message.
    print("  checking engine ...", end="", flush=True)
    try:
        probe = chess.engine.SimpleEngine.popen_uci(engine_cmd, debug=False)
        engine_name = probe.id.get("name", "?")
        probe.quit()
        print(f" OK ({engine_name})")
    except Exception as ex:
        print(f" FAILED: {ex}")
        print()
        print("  Hints:")
        print("  - use ASCII quotes only (\") — typographic quotes (\" \") break the command")
        print("  - quote the WHOLE command once:  --engine \"java -jar /path/engine.jar\"")
        print("  - check that the engine path/file actually exists")
        sys.exit(1)

    print("=" * 70)
    print("WAC test runner")
    print("=" * 70)
    print(f"  engine     : {engine_cmd}")
    print(f"  epd        : {args.epd}")
    print(f"  positions  : {args.positions or 'all'}")
    print(f"  workers    : {args.workers}")
    print(f"  hash       : {args.hash} MB per engine")
    print(f"  movetime   : {args.movetime} ms")
    if args.setoption:
        print(f"  setoption  : {', '.join(args.setoption)}")
    print()

    tasks = load_positions(args.epd, args.positions)
    total = len(tasks)
    if total == 0:
        print("ERROR: no positions loaded")
        sys.exit(1)
    print(f"  loaded {total} positions")
    print()

    extra_options = {}
    for entry in args.setoption:
        if "=" not in entry:
            print(f"ERROR: --setoption must be NAME=VALUE, got: {entry}")
            sys.exit(1)
        name, _, value = entry.partition("=")
        extra_options[name.strip()] = value.strip()

    pool = EnginePool(engine_cmd, args.hash, extra_options)
    solved = 0
    failures = []
    start = time.time()
    done = 0

    try:
        with ThreadPoolExecutor(max_workers=args.workers) as ex:
            futures = {ex.submit(solve_task, pool, t, args.movetime / 1000.0): t for t in tasks}
            for fut in as_completed(futures):
                task, got = fut.result()
                done += 1
                ok = got in task["solutions"]
                if ok:
                    solved += 1
                else:
                    failures.append((task, got))
                status = "solved" if ok else "FAILED"
                print(f"\r  [{done}/{total}] {status:>6}: {task['id']:<10} "
                      f"(expected {','.join(task['solutions'])}, got {got})", end="")
    except KeyboardInterrupt:
        print("\n\n  interrupted!")
    print()

    elapsed = time.time() - start
    pct = 100.0 * solved / done if done else 0.0

    print()
    print("=" * 70)
    print(f"  RESULT: {solved}/{done} solved  ({pct:.1f} %)  in {elapsed:.1f} s")
    print("=" * 70)

    if failures:
        print(f"\n  failed positions ({len(failures)}):")
        for task, got in failures:
            print(f"    {task['id']:<10} expected {','.join(task['solutions']):<10} "
                  f"got {got:<10} {task['fen']}")

    pool.close_all()
    sys.exit(0 if solved == done else 2)


if __name__ == "__main__":
    main()
