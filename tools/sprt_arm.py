#!/usr/bin/env python3
"""Helpers for the lever test campaign (docs/audit-tests-KB.md).

Three commands, all taking the campaign binary from --engine or from the
KB_ENGINE environment variable:

    python tools/sprt_arm.py arm --id P1_01_UseProbCut --arm "UseProbCut=true"
    python tools/sprt_arm.py bench --arm "UseProbCut=true"
    python tools/sprt_arm.py check

`arm` plays the arm ("ON": the lever options set) against the same binary with
the defaults ("OFF"), one lever per match, with the campaign standard: 400
games, 10+0.1, one thread, 256 MB, UHO openings in random order, seed 1234,
SPRT 0 -> 5 with bounds of about +/-2 LLR, Move Overhead 10 on both sides. It
writes matches/<id>.log (the fastchess output), matches/<id>.pgn and
matches/<id>.result.txt (the verbatim result block and the cleanliness check),
kills fastchess after --hard-minutes and prints a summary.

A knob that only acts with its switch (ProbCutMargin needs UseProbCut) is run
with --base "UseProbCut=true": both sides get the base options and only the
knob differs, so the match still changes one variable.

`bench` prints the fixed-depth pre-flight total of an arm; it must differ from
the total of the base alone, otherwise the lever is not wired.

`check` prints the session checks of section 1.6 of the document: the binary's
sha256, the number of handshake options (70) and the default bench total.
"""

import argparse
import hashlib
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# SPRT bounds. fastchess accepts alpha and beta; with alpha = beta = 0.12 the
# log-likelihood bounds are ln(0.88 / 0.12) = +/-1.99, the +/-2 of the decision
# rule in docs/audit-tests-KB.md.
SPRT_ALPHA = 0.12
SPRT_BETA = 0.12

DEFAULT_BENCH = 456_308
HANDSHAKE_OPTIONS = 70


def fastchess_path():
    configured = os.environ.get("KB_FASTCHESS")
    if configured:
        return configured
    windows_default = r"C:\dev\fastchess-windows-x86-64\fastchess.exe"
    if os.path.exists(windows_default):
        return windows_default
    found = shutil.which("fastchess")
    if found:
        return found
    raise SystemExit("fastchess not found: set KB_FASTCHESS")


def book_path():
    configured = os.environ.get("KB_BOOK")
    path = Path(configured) if configured else REPO / "books" / "UHO_Lichess_4852_v1.epd"
    if not path.exists():
        raise SystemExit(f"opening book not found: {path} (set KB_BOOK)")
    return path


def engine_path(engine):
    engine = engine or os.environ.get("KB_ENGINE")
    if not engine or not Path(engine).exists():
        raise SystemExit("give --engine or set KB_ENGINE to the campaign binary")
    return str(Path(engine).resolve())


def split_options(text):
    """'UseProbCut=true ProbCutMargin=180' -> ['UseProbCut=true', 'ProbCutMargin=180']."""
    options = text.split()
    for option in options:
        if not re.fullmatch(r"[A-Za-z0-9]+=\S+", option):
            raise SystemExit(f"bad option '{option}' (expected Name=value)")
    return options


def arm_bench(engine, arm_options, base_options="", depth=12):
    """The `bench <depth>` node total with the arm (and its base) configured."""
    overrides = [f"param:{option}" for option in split_options(f"{base_options} {arm_options}")]
    result = subprocess.run(
        [engine, *overrides],
        input=f"bench {depth}\nquit\n",
        capture_output=True,
        text=True,
        timeout=600,
    )
    for line in result.stdout.splitlines():
        if line.startswith("Nodes:"):
            return int(line.split(":")[1].strip())
    raise SystemExit(f"bench printed no total for '{arm_options}': {result.stdout[:200]}")


def handshake_options(engine):
    output = subprocess.run([engine], input="uci\nquit\n", capture_output=True, text=True, timeout=60).stdout
    return sum(1 for line in output.splitlines() if line.startswith("option name "))


def file_sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def kill_engines(engine):
    """Best effort: stop engine processes that outlived a killed fastchess."""
    name = Path(engine).name
    if os.name == "nt":
        subprocess.run(["taskkill", "/F", "/IM", name], capture_output=True)
    else:
        subprocess.run(["pkill", "-f", name], capture_output=True)


def run_arm(
    arm_id,
    arm_options,
    base_options="",
    engine=None,
    games=400,
    tc="10+0.1",
    concurrency=6,
    hash_mb=256,
    overhead=10,
    seed=1234,
    out_dir="matches",
    hard_minutes=360,
    sprt=True,
):
    """Plays one arm and returns the result block as a dictionary."""
    engine = engine_path(engine)
    book = book_path()
    out = Path(out_dir).resolve()
    out.mkdir(parents=True, exist_ok=True)
    log, pgn, result_file = out / f"{arm_id}.log", out / f"{arm_id}.pgn", out / f"{arm_id}.result.txt"
    for path in (log, pgn, result_file):
        path.unlink(missing_ok=True)

    common = [f"option.Hash={hash_mb}", f"option.Move Overhead={overhead}"]
    base = [f"option.{option}" for option in split_options(base_options)]
    arm = [f"option.{option}" for option in split_options(arm_options)]
    command = [
        fastchess_path(),
        "-engine", "name=ON", f"cmd={engine}", *common, *base, *arm,
        "-engine", "name=OFF", f"cmd={engine}", *common, *base,
        "-each", f"tc={tc}",
        "-rounds", str(games // 2), "-games", "2", "-repeat",
        "-concurrency", str(concurrency),
        "-openings", f"file={book}", "format=epd", "order=random", "-srand", str(seed),
        "-resign", "movecount=3", "score=600", "-draw", "movenumber=34", "movecount=8", "score=20",
        "-pgnout", f"file={pgn}", "-report", "penta=true", "-ratinginterval", "20",
    ]
    if sprt:
        command += ["-sprt", "elo0=0", "elo1=5", f"alpha={SPRT_ALPHA}", f"beta={SPRT_BETA}", "model=normalized"]

    # fastchess drops a config.json into its working directory: run it in the
    # output folder, not in the repository.
    finished = True
    try:
        completed = subprocess.run(
            command, cwd=out, capture_output=True, text=True, timeout=hard_minutes * 60
        )
        text = completed.stdout + completed.stderr
        exit_code = completed.returncode
    except subprocess.TimeoutExpired as expired:
        finished = False
        exit_code = "killed"
        text = (expired.stdout or b"").decode("utf8", "replace") + (expired.stderr or b"").decode("utf8", "replace")
        kill_engines(engine)
    log.write_text(text, encoding="utf8")

    lines = text.splitlines()

    def last(prefix):
        matches = [line for line in lines if line.startswith(prefix)]
        return matches[-1] if matches else ""

    time_losses = sum(1 for line in lines if re.search(r"loses on time|time forfeit", line, re.I))
    abnormal = sum(
        1
        for line in lines
        if re.search(r"illegal move|disconnect|crashed|stalls", line, re.I) and "Crashed: 0" not in line
    )
    llr_lines = [line for line in lines if "LLR" in line]
    block = {
        "ID": arm_id,
        "Base": base_options,
        "Arm": arm_options,
        "Engine": engine,
        "Finished": f"{finished} (exit code {exit_code})",
        "Score": last("Games:"),
        "Elo": last("Elo:"),
        "LOS": last("LOS:"),
        "SPRT": llr_lines[-1] if llr_lines else "",
        "Time loss": str(time_losses),
        "Abnormal": f"{abnormal} (illegal move, disconnect, crash, stall)",
    }
    text_block = "\n".join(f"{key:<10}: {value}" for key, value in block.items())
    result_file.write_text(text_block + "\n", encoding="utf8")
    print(text_block)
    if time_losses or abnormal or not finished:
        print(
            f"WARNING: {arm_id} was NOT clean: re-run it (docs/audit-tests-KB.md section 0.1) "
            "before using the result.",
            file=sys.stderr,
        )
    block["clean"] = not (time_losses or abnormal or not finished)
    return block


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)

    arm = sub.add_parser("arm", help="play one SPRT arm")
    arm.add_argument("--id", required=True, help="for example P1_01_UseProbCut")
    arm.add_argument("--arm", required=True, help='lever options, for example "UseProbCut=true"')
    arm.add_argument("--base", default="", help="options set on both sides (a knob's switch)")
    arm.add_argument("--engine")
    arm.add_argument("--games", type=int, default=400)
    arm.add_argument("--tc", default="10+0.1")
    arm.add_argument("--concurrency", type=int, default=6)
    arm.add_argument("--hash", type=int, default=256, dest="hash_mb")
    arm.add_argument("--overhead", type=int, default=10)
    arm.add_argument("--seed", type=int, default=1234)
    arm.add_argument("--out-dir", default="matches")
    arm.add_argument("--hard-minutes", type=int, default=360)
    arm.add_argument("--no-sprt", action="store_true", help="play all games, no early stop")

    bench = sub.add_parser("bench", help="fixed-depth pre-flight total of an arm")
    bench.add_argument("--arm", required=True)
    bench.add_argument("--base", default="")
    bench.add_argument("--depth", type=int, default=12)
    bench.add_argument("--engine")

    check = sub.add_parser("check", help="binary hash, handshake count, default bench")
    check.add_argument("--engine")

    args = parser.parse_args()
    if args.command == "arm":
        result = run_arm(
            args.id, args.arm, args.base, args.engine, args.games, args.tc, args.concurrency,
            args.hash_mb, args.overhead, args.seed, args.out_dir, args.hard_minutes, not args.no_sprt,
        )
        return 0 if result["clean"] else 1
    if args.command == "bench":
        engine = engine_path(args.engine)
        total = arm_bench(engine, args.arm, args.base, args.depth)
        base = arm_bench(engine, args.base or "UseStableTimeManagement=true", "", args.depth)
        print(f"bench {args.depth}: {total:,} with the arm; {base:,} for the base ({'differs' if total != base else 'IDENTICAL: not wired, unless it is UseStableTimeManagement, which only a clock sees'})")
        return 0
    engine = engine_path(args.engine)
    options = handshake_options(engine)
    default = arm_bench(engine, "UseStableTimeManagement=true")
    print(f"engine   : {engine}")
    print(f"sha256   : {file_sha256(engine)}")
    print(f"handshake: {options} options ({'ok' if options == HANDSHAKE_OPTIONS else f'expected {HANDSHAKE_OPTIONS}'})")
    print(f"bench 12 : {default:,} ({'ok' if default == DEFAULT_BENCH else f'expected {DEFAULT_BENCH:,}'})")
    return 0 if options == HANDSHAKE_OPTIONS and default == DEFAULT_BENCH else 1


if __name__ == "__main__":
    sys.exit(main())
