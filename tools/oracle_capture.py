#!/usr/bin/env python3
"""Capture machine-readable golden data from a UCI reference engine.

The script is intentionally engine-agnostic: pass the local reference binary
with ``--engine``. It records:

* deterministic benchmark totals for depths 12, 16 and 18;
* perft counts for the standard test positions;
* raw network evaluations for the shared FEN list;
* the UCI handshake and a few lifecycle transcripts.

The output is derived data only; the reference source tree and binary are not
copied into the project.
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import queue
import re
import subprocess
import sys
import threading
import time
from pathlib import Path

# Standard perft positions. Depths are chosen to keep the capture quick while
# still exercising castling, en passant, promotions, checks and pins.
PERFT_CASES = [
    ("startpos", "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", 4),
    ("kiwipete", "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1", 3),
    ("position3", "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1", 4),
    ("position4", "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1", 3),
    ("position5", "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8", 3),
]

# Shared FEN list for the network evaluation parity tests. It varies side to
# move, king square/region, mirror state and material count.
NNUE_FENS = [
    ("startpos", "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"),
    ("kiwipete", "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"),
    ("position3", "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1"),
    ("position4", "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1"),
    ("position5", "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8"),
    ("position6", "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10"),
    ("kiwipete_black", "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R b KQkq - 0 1"),
    ("kings_deep", "4k3/8/8/8/8/8/4P3/4K3 b - - 0 1"),
    ("king_a1_white", "8/8/8/8/8/8/8/K6k w - - 0 1"),
    ("king_a1_black", "8/8/8/8/8/8/8/K6k b - - 0 1"),
    ("king_a8_black", "K6k/8/8/8/8/8/8/8 b - - 0 1"),
    ("castling_rook", "4k3/8/8/8/8/8/8/4K2R w K - 0 1"),
    ("castling_rook_no_rights", "8/8/8/8/8/8/6k1/4K2R w K - 0 1"),
    ("uho_opening", "r2q1rk1/pp1bppbp/2np1np1/2p5/P1B1PB2/3P1N2/1PP2PPP/RN1Q1RK1 w - - 1 9"),
    ("sharp_castled", "r1bq1rk1/p1pnbppp/1p3n2/3p2B1/3P4/2N1PN2/PPQ2PPP/R3KB1R w KQ - 0 9"),
]


def run(engine: Path, commands: str, timeout: int = 180) -> str:
    """Runs one command batch and returns stdout."""
    result = subprocess.run(
        [str(engine)],
        input=commands,
        text=True,
        capture_output=True,
        timeout=timeout,
        cwd=str(engine.parent),
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"reference engine failed ({result.returncode}): {result.stderr.strip()}"
        )
    return result.stdout


def _number(text: str) -> int:
    """Normalizes a localized integer such as ``3.169.859`` or ``3,169,859``."""
    return int(text.replace(".", "").replace(",", ""))


def capture_bench(engine: Path, depths: list[int]) -> dict:
    """Captures the deterministic benchmark totals for each requested depth."""
    captures = {}
    for depth in depths:
        output = run(engine, f"bench {depth} 1\nquit\n")
        nodes = re.search(r"Total Nodes:\s*([\d.,]+)", output)
        usec = re.search(r"Total Time:\s*([\d.,]+)\s*ms", output)
        nps = re.search(r"Average NPS:\s*([\d.,]+)", output)
        if not nodes:
            raise RuntimeError(f"bench {depth}: no total node count found")
        captures[str(depth)] = {
            "total_nodes": _number(nodes.group(1)),
            "total_time_ms": _number(usec.group(1)) if usec else None,
            "average_nps": _number(nps.group(1)) if nps else None,
        }
    return captures


def capture_perft(engine: Path) -> list[dict]:
    """Captures perft counts for the standard positions."""
    rows = []
    for name, fen, depth in PERFT_CASES:
        output = run(engine, f"position fen {fen}\nperft {depth}\nquit\n")
        match = re.search(r"Nodes:\s*([\d,]+)", output)
        if not match:
            raise RuntimeError(f"perft {name} depth {depth}: no node count found")
        rows.append(
            {
                "name": name,
                "fen": fen,
                "depth": depth,
                "nodes": _number(match.group(1)),
            }
        )
    return rows


def capture_nnue(engine: Path) -> list[dict]:
    """Captures the raw network evaluation for each shared FEN."""
    rows = []
    for name, fen in NNUE_FENS:
        output = run(engine, f"position fen {fen}\nnnueeval\nquit\n")
        match = re.search(r"nnueeval\s+(-?\d+)", output)
        if not match:
            raise RuntimeError(f"nnueeval {name}: no evaluation found")
        rows.append({"name": name, "fen": fen, "cp": int(match.group(1))})
    return rows


def parse_option(line: str) -> dict:
    """Parses one UCI ``option name ...`` line into a structured record."""
    body = line[len("option name ") :]
    name, _, rest = body.partition(" type ")
    parts = rest.split()
    option_type = parts[0] if parts else ""
    default = None
    minimum = None
    maximum = None

    if "default" in parts:
        index = parts.index("default")
        if index + 1 < len(parts) and parts[index + 1] not in ("min", "max"):
            default = parts[index + 1]
        else:
            default = ""
    if "min" in parts:
        minimum = int(parts[parts.index("min") + 1])
    if "max" in parts:
        maximum = int(parts[parts.index("max") + 1])

    return {
        "name": name,
        "type": option_type,
        "default": default,
        "min": minimum,
        "max": maximum,
    }


def capture_uci(engine: Path) -> dict:
    """Captures the sanitized handshake, option catalog and lifecycle tokens."""
    handshake = run(engine, "uci\nisready\nucinewgame\nquit\n")
    unknown = run(engine, "this-command-does-not-exist\nquit\n")
    options = [
        parse_option(line.strip())
        for line in handshake.splitlines()
        if line.strip().startswith("option name ")
    ]
    responses = [
        line.strip()
        for line in handshake.splitlines()
        if line.strip() in ("uciok", "readyok")
    ]
    unknown_response = next(
        (line.strip() for line in unknown.splitlines() if "Unknown command" in line),
        "",
    )
    return {
        "options": options,
        "responses": responses,
        "unknown": unknown_response,
    }


# ---------------------------------------------------------------------------
# Phase 5 step 0: interactive UCI transcript capture
#
# The transcripts freeze the reference engine's protocol behaviour before the
# protocol code is written (migration plan, Phase 5 step 0). Each scenario runs
# in a fresh engine session; sent commands are recorded as "> " lines so the
# file is a readable request/response transcript. Published files contain no
# engine identity: id lines and the start-up banner become placeholders, and
# wall-clock-dependent fields ("time", "nps", "hashfull", "tbhits", and where
# noted "nodes") are masked.
# ---------------------------------------------------------------------------

# Fields whose values depend on wall-clock timing or host speed; they cannot
# be compared between runs and are masked in every published transcript.
_TIMING_MASKS = [
    (re.compile(r"\btime \d+"), "time <ms>"),
    (re.compile(r"\bnps \d+"), "nps <nps>"),
    (re.compile(r"\bhashfull \d+"), "hashfull <permille>"),
    (re.compile(r"\btbhits \d+"), "tbhits <hits>"),
]


def sanitize(lines: list[str], mask_nodes: bool = False) -> list[str]:
    """Masks identity and timing fields so a transcript can be published."""
    out: list[str] = []
    for raw in lines:
        line = raw.rstrip("\n")
        if line.startswith("id name "):
            line = "id name <engine>"
        elif line.startswith("id author "):
            line = "id author <author>"
        elif "eval-source:" in line:
            # The eval-source line names the reference's network file; the
            # net identity is not published, only the line's existence.
            line = re.sub(r"eval-source: .*", "eval-source: <nnue net>", line)
        elif line.strip().startswith("*"):
            # The start-up banner block collapses into one placeholder line.
            if out and out[-1] == "<start-up banner>":
                continue
            line = "<start-up banner>"
        for pattern, replacement in _TIMING_MASKS:
            line = pattern.sub(replacement, line)
        if mask_nodes:
            line = re.sub(r"\bnodes \d+", "nodes <nodes>", line)
        out.append(line)
    return out


class EngineSession:
    """Runs the reference engine and exchanges UCI lines with it.

    A background reader thread feeds stdout lines into a queue so the capture
    can interleave timed commands (``stop``, ``ponderhit``, ``isready``) with
    an ongoing search. The engine's working directory is its binary's folder
    so it finds its network file next to the executable.
    """

    def __init__(self, engine: Path):
        self._process = subprocess.Popen(
            [str(engine)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            encoding="utf-8",
            errors="replace",
            cwd=str(engine.parent),
        )
        self._lines: queue.Queue[str | None] = queue.Queue()
        self._reader = threading.Thread(target=self._read_stdout, daemon=True)
        self._reader.start()

    def _read_stdout(self) -> None:
        """Pumps engine output into the queue until the stream ends."""
        assert self._process.stdout is not None
        for raw in self._process.stdout:
            self._lines.put(raw.rstrip("\n"))
        self._lines.put(None)

    def send(self, line: str) -> None:
        """Sends one command line; records a dead engine instead of raising."""
        assert self._process.stdin is not None
        try:
            self._process.stdin.write(line + "\n")
            self._process.stdin.flush()
        except OSError:
            # The engine's stdin pipe broke (the process died or closed its
            # input). Record the fact; the transcript stays honest.
            code = self._process.poll()
            self.transcript.append(f"# engine stdin broken (process exit code {code})")

    def command(self, line: str) -> None:
        """Sends one command and records it as a "> " transcript line."""
        self.transcript.append(f"> {line}")
        self.send(line)

    def collect(self, seconds: float) -> list[str]:
        """Records engine output for a fixed wall-clock window."""
        collected: list[str] = []
        deadline = time.monotonic() + seconds
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break
            try:
                line = self._lines.get(timeout=min(remaining, 0.05))
            except queue.Empty:
                continue
            if line is None:
                break
            collected.append(line)
        self.transcript.extend(collected)
        return collected

    def collect_until(self, marker: str, timeout: float = 60.0) -> list[str]:
        """Records output until a line starts with ``marker`` (e.g. bestmove).

        If the engine process dies first, the death is recorded and the wait
        ends; a crashed engine will never print its marker.
        """
        collected: list[str] = []
        start = time.monotonic()
        while True:
            if time.monotonic() - start > timeout:
                raise RuntimeError(f"engine never printed {marker!r}")
            if self._process.poll() is not None:
                self.transcript.append(
                    f"# engine exited (code {self._process.returncode:#x}) before {marker!r}"
                )
                break
            try:
                line = self._lines.get(timeout=0.05)
            except queue.Empty:
                continue
            if line is None:
                break
            collected.append(line)
            if line.startswith(marker):
                break
        self.transcript.extend(collected)
        return collected

    def collect_idle(self, quiet: float = 0.35, timeout: float = 30.0) -> list[str]:
        """Records output until the engine has been silent for ``quiet`` seconds.

        A dead engine is also "silent"; the death is recorded once.
        """
        collected: list[str] = []
        start = time.monotonic()
        last = time.monotonic()
        death_noted = False
        while True:
            if time.monotonic() - start > timeout:
                raise RuntimeError("engine did not fall silent")
            if not death_noted and self._process.poll() is not None:
                self.transcript.append(
                    f"# engine exited (code {self._process.returncode:#x})"
                )
                death_noted = True
            try:
                line = self._lines.get(timeout=0.05)
            except queue.Empty:
                if time.monotonic() - last >= quiet:
                    break
                continue
            if line is None:
                break
            collected.append(line)
            last = time.monotonic()
        self.transcript.extend(collected)
        return collected

    def note(self, text: str) -> None:
        """Adds a derived measurement as a comment line to the transcript."""
        self.transcript.append(f"# {text}")

    def close(self) -> None:
        """Ends the engine cleanly; falls back to killing it."""
        try:
            self.send("quit")
            self._process.wait(timeout=5)
        except Exception:
            self._process.kill()

    def __enter__(self) -> "EngineSession":
        self.transcript: list[str] = []
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()


# Back-rank mate-in-one fixture for the "go mate" probe (the rook mate has no
# escape square and nothing to block with).
_MATE_IN_ONE = "6k1/5ppp/8/8/8/8/5PPP/R5K1 w - - 0 1"


def _scenario_handshake(engine: Path) -> list[str]:
    """Handshake: banner, uci, isready, ucinewgame, isready."""
    with EngineSession(engine) as session:
        session.collect_idle()  # the start-up banner arrives before any command
        session.command("uci")
        session.collect_idle()
        session.command("isready")
        session.collect_idle()
        session.command("ucinewgame")
        session.collect_idle()
        session.command("isready")
        session.collect_idle()
    return sanitize(session.transcript)


def _scenario_go_limiter_depth(engine: Path) -> list[str]:
    """Deterministic go limiters: node counts are kept unmasked."""
    with EngineSession(engine) as session:
        session.collect(0.4)  # swallow the banner
        session.command("position startpos")
        session.note("a) bare go (no limiter)")
        session.command("go")
        session.collect_until("bestmove")
        session.note("b) go depth 7")
        session.command("go depth 7")
        session.collect_until("bestmove")
        session.note("c) go nodes 50000")
        session.command("go nodes 50000")
        session.collect_until("bestmove")
        session.note("d) go movestogo 40 alone")
        session.command("go movestogo 40")
        session.collect_until("bestmove")
        session.note("e) go perft 3")
        session.command("go perft 3")
        session.collect_idle()
    return sanitize(session.transcript)


def _scenario_go_limiter_time(engine: Path) -> list[str]:
    """Time-limited go calls; node counts are masked (host-dependent)."""
    with EngineSession(engine) as session:
        session.collect(0.4)
        session.command("position startpos")
        session.note("a) go movetime 250")
        session.command("go movetime 250")
        session.collect_until("bestmove")
        session.note("b) go wtime/btime with increments")
        session.command("go wtime 1000 btime 1000 winc 10 binc 10")
        session.collect_until("bestmove")
        session.note("c) go wtime/btime with movestogo")
        session.command("go wtime 1000 btime 1000 winc 10 binc 10 movestogo 20")
        session.collect_until("bestmove")
    return sanitize(session.transcript, mask_nodes=True)


def _scenario_go_infinite_stop(engine: Path) -> list[str]:
    """go infinite, stopped while it is running."""
    with EngineSession(engine) as session:
        session.collect(0.4)
        session.command("position startpos")
        session.command("go infinite")
        session.collect(0.7)
        session.command("stop")
        session.collect_until("bestmove")
    return sanitize(session.transcript, mask_nodes=True)


def _scenario_stop(engine: Path) -> list[str]:
    """stop in the four situations the plan asks about."""
    with EngineSession(engine) as session:
        session.collect(0.4)
        session.command("position startpos")
        session.note("a) stop with no search running")
        session.command("stop")
        session.collect_idle()
        session.note("b) stop sent immediately after go depth 1")
        session.command("go depth 1")
        session.command("stop")
        session.collect_until("bestmove")
        session.note("c) stop during a deep search")
        session.command("go depth 14")
        session.collect(0.6)
        session.command("stop")
        session.collect_until("bestmove")
        session.note("d) stop after a search has finished")
        session.command("go depth 5")
        session.collect_until("bestmove")
        session.command("stop")
        session.collect_idle()
    return sanitize(session.transcript, mask_nodes=True)


def _scenario_ponder(engine: Path) -> list[str]:
    """go ponder with stop and ponderhit, plus a stray ponderhit."""
    with EngineSession(engine) as session:
        session.collect(0.4)
        session.command("position startpos")
        session.note("a) go ponder, then stop without ponderhit")
        session.command("go ponder wtime 5000 btime 5000 winc 100 binc 100")
        session.collect(0.6)
        session.command("stop")
        session.collect_until("bestmove")
        session.note("b) go ponder, then ponderhit (the clock starts at the ponderhit moment)")
        session.command("go ponder wtime 5000 btime 5000 winc 100 binc 100")
        session.collect(0.6)
        started = time.monotonic()
        session.command("ponderhit")
        session.collect_until("bestmove", timeout=120)
        session.note(f"elapsed ponderhit -> bestmove: {time.monotonic() - started:.2f} s")
        session.note("c) ponderhit with no ponder search running")
        session.command("ponderhit")
        session.collect_idle()
    return sanitize(session.transcript, mask_nodes=True)


def _scenario_mate_searchmoves(engine: Path) -> list[str]:
    """go mate and go searchmoves behaviour."""
    with EngineSession(engine) as session:
        session.collect(0.4)
        session.command(f"position fen {_MATE_IN_ONE}")
        session.note("a) go mate 1 on a mate-in-one position")
        session.command("go mate 1")
        session.collect_until("bestmove", timeout=60)
        session.command("position startpos")
        session.note("b) go mate 3 on the start position (stopped after 2 s if unresolved)")
        session.command("go mate 3")
        window = session.collect(2.0)
        if not any(line.startswith("bestmove") for line in window):
            session.command("stop")
            session.collect_until("bestmove")
        session.command("position startpos")
        session.note("c) go depth 5 searchmoves e2e4")
        session.command("go depth 5 searchmoves e2e4")
        session.collect_until("bestmove")
        session.note("d) go depth 5 searchmoves e2e4 d2d4")
        session.command("go depth 5 searchmoves e2e4 d2d4")
        session.collect_until("bestmove")
        session.note("e) go depth 5 searchmoves (no moves given)")
        session.command("go depth 5 searchmoves")
        session.collect_until("bestmove")
    return sanitize(session.transcript)


def _scenario_ucinewgame(engine: Path) -> list[str]:
    """ucinewgame must reset to the start position (predecessor bug 143 fix)."""
    with EngineSession(engine) as session:
        session.collect(0.4)
        session.command("position startpos moves e2e4")
        session.command("ucinewgame")
        session.collect_idle()
        session.note("go depth 1 after ucinewgame must search the start position")
        session.command("go depth 1")
        session.collect_until("bestmove")
    return sanitize(session.transcript)


def _scenario_setoption(engine: Path) -> list[str]:
    """setoption validation: valid, out-of-range, unknown, button, check."""
    with EngineSession(engine) as session:
        session.collect(0.4)
        checks = [
            ("a) valid spin value", "setoption name Hash value 32"),
            ("b) spin above its maximum", "setoption name Hash value 999999"),
            ("c) spin below its minimum", "setoption name Hash value 0"),
            ("d) unknown option name", "setoption name NoSuchOption value 5"),
            ("e) check option, valid value", "setoption name Own Book value true"),
            ("f) check option, valid value", "setoption name Own Book value false"),
            ("g) check option, invalid value", "setoption name Own Book value maybe"),
            ("h) button option without a value", "setoption name Clear Hash"),
            ("i) Threads below its minimum", "setoption name Threads value 0"),
            ("j) Threads above its maximum", "setoption name Threads value 999"),
        ]
        for note, command in checks:
            session.note(note)
            session.command(command)
            session.collect_idle()
    return sanitize(session.transcript)


def _scenario_isready(engine: Path) -> list[str]:
    """isready must be answered at once while a search is running."""
    with EngineSession(engine) as session:
        session.collect(0.4)
        session.command("position startpos")
        session.command("go depth 16")
        session.collect(0.4)
        session.note("isready sent while the search is running")
        sent = time.monotonic()
        session.command("isready")
        session.collect_until("readyok", timeout=10)
        session.note(f"readyok arrived after {time.monotonic() - sent:.3f} s")
        session.command("stop")
        session.collect_until("bestmove")
    return sanitize(session.transcript, mask_nodes=True)


def _scenario_threads(engine: Path) -> list[str]:
    """Which thread's result does a multi-thread search report?"""
    with EngineSession(engine) as session:
        session.collect(0.4)
        session.command("position startpos")
        session.note("single-thread reference run: go depth 12")
        session.command("go depth 12")
        lines = session.collect_until("bestmove")
        depth_lines = [l for l in lines if l.startswith("info depth")]
        match = re.search(r"nodes (\d+)", depth_lines[-1]) if depth_lines else None
        if match:
            session.note(f"single-thread nodes at depth 12: {match.group(1)}")
        session.command("setoption name Threads value 4")
        session.collect_idle()
        session.note("four threads: go depth 12")
        session.command("go depth 12")
        lines = session.collect_until("bestmove", timeout=120)
        depth_lines = [l for l in lines if l.startswith("info depth")]
        match = re.search(r"nodes (\d+)", depth_lines[-1]) if depth_lines else None
        if match:
            session.note(f"reported nodes at depth 12 with 4 threads: {match.group(1)}")
        session.command("setoption name Threads value 1")
        session.collect_idle()
    return sanitize(session.transcript, mask_nodes=True)


# The published scenario set, in capture order. The title appears in the file
# header and must not name the reference engine.
TRANSCRIPT_SCENARIOS = [
    ("handshake", _scenario_handshake, "UCI handshake, isready and ucinewgame"),
    ("go_limiter_depth", _scenario_go_limiter_depth,
     "go with deterministic limiters (none, depth, nodes, movestogo, perft)"),
    ("go_limiter_time", _scenario_go_limiter_time,
     "go with time limiters (movetime, clocks, movestogo)"),
    ("go_infinite_stop", _scenario_go_infinite_stop,
     "go infinite stopped while running"),
    ("stop", _scenario_stop, "stop with no search, during depth 1, mid-search, after a search"),
    ("ponder", _scenario_ponder, "go ponder with stop and ponderhit, stray ponderhit"),
    ("mate_searchmoves", _scenario_mate_searchmoves, "go mate and go searchmoves"),
    ("ucinewgame", _scenario_ucinewgame, "ucinewgame resets to the start position"),
    ("setoption", _scenario_setoption, "setoption validation"),
    ("isready_during_search", _scenario_isready, "isready answered during a search"),
    ("threads", _scenario_threads, "which thread reports the result with Threads=4"),
]


def capture_transcripts(engine: Path, out_dir: Path) -> None:
    """Runs every scenario and writes the sanitized transcript files."""
    out_dir.mkdir(parents=True, exist_ok=True)
    today = datetime.date.today().isoformat()
    for name, scenario, title in TRANSCRIPT_SCENARIOS:
        lines = scenario(engine)
        path = out_dir / f"{name}.txt"
        with path.open("w", encoding="utf-8", newline="\n") as handle:
            handle.write(f"# UCI transcript: {title}\n")
            handle.write(f"# Captured {today} (migration plan, Phase 5 step 0). Identity lines are\n")
            handle.write("# placeholders; time, nps, hashfull, tbhits and (where noted) nodes\n")
            handle.write("# values are masked. Lines starting with '>' are commands sent to the\n")
            handle.write("# engine, '#' lines are derived measurements, the rest is engine output.\n")
            for line in lines:
                handle.write(line + "\n")
        print(f"wrote {path}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", required=True, help="reference UCI binary")
    parser.add_argument(
        "--out-dir",
        default="tests/golden",
        help="directory for the derived JSON files (default: tests/golden)",
    )
    parser.add_argument(
        "--bench-depths",
        default="12,16,18",
        help="comma-separated benchmark depths (default: 12,16,18)",
    )
    parser.add_argument(
        "--transcripts",
        action="store_true",
        help="also capture the Phase 5 step 0 UCI transcripts into <out-dir>/transcripts",
    )
    parser.add_argument(
        "--transcripts-only",
        action="store_true",
        help="capture only the UCI transcripts, skipping the golden JSON files",
    )
    args = parser.parse_args()

    engine = Path(args.engine).resolve()
    if not engine.is_file():
        print(f"error: engine not found: {engine}", file=sys.stderr)
        return 1

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    depths = [int(value) for value in args.bench_depths.split(",") if value]

    if args.transcripts or args.transcripts_only:
        capture_transcripts(engine, out_dir / "transcripts")
        if args.transcripts_only:
            return 0

    data = {
        "bench": capture_bench(engine, depths),
        "perft": capture_perft(engine),
        "nnue": capture_nnue(engine),
        "uci": capture_uci(engine),
    }

    for name, payload in data.items():
        path = out_dir / f"{name}.json"
        with path.open("w", encoding="utf-8") as handle:
            json.dump(payload, handle, indent=2)
            handle.write("\n")
        print(f"wrote {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
