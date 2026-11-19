#!/usr/bin/env python3
"""Differential tablebase check over random positions.

Draws random legal positions whose material exists in a Syzygy directory and
compares, for every position:

* the engine's root tablebase answer (``SyzygyRootProbe``: WDL class and DTZ),
* an independent tablebase reader (the ``chess`` package), and
* optionally a second engine binary (``--other``) speaking the same UCI
  dialect, which must also agree on the chosen move.

Positions where the side to move is mated or stalemated are skipped (the
engine answers ``bestmove 0000`` there). Positions are generated from a fixed
seed, so a failure is reproducible with the same ``--seed``.

Usage::

    python tools/tb_compare.py --engine target/release/knightbyte.exe \
        --tb-path C:/tb/Syzygy-3-4-5 --samples 1500 [--other other.exe]
"""

import argparse
import os
import random
import subprocess
import sys
import time

import chess
import chess.syzygy


def material_names(tb_path):
    """Returns the material names (``KQPvKR``) that have a WDL file."""
    names = []
    for entry in os.listdir(tb_path):
        if entry.endswith(".rtbw") and os.path.exists(
            os.path.join(tb_path, entry[:-5] + ".rtbz")
        ):
            names.append(entry[:-5])
    return sorted(names)


def random_position(name, rng):
    """Builds a random legal position for a material name, or None."""
    white, black = name.split("v")
    for _ in range(200):
        board = chess.Board(None)
        squares = rng.sample(range(64), len(white) + len(black))
        pieces = list(white) + list(black)
        ok = True
        for index, (symbol, square) in enumerate(zip(pieces, squares)):
            color = chess.WHITE if index < len(white) else chess.BLACK
            piece_type = chess.PIECE_SYMBOLS.index(symbol.lower())
            if piece_type == chess.PAWN and chess.square_rank(square) in (0, 7):
                ok = False
                break
            board.set_piece_at(square, chess.Piece(piece_type, color))
        if not ok:
            continue
        board.turn = rng.choice([chess.WHITE, chess.BLACK])
        board.halfmove_clock = 0
        if not board.is_valid():
            continue
        if board.is_game_over():
            continue
        return board
    return None


class Engine:
    """A minimal UCI session."""

    def __init__(self, path):
        self.process = subprocess.Popen(
            [path],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            bufsize=1,
        )

    def send(self, line):
        self.process.stdin.write(line + "\n")
        self.process.stdin.flush()

    def read_until(self, prefix, timeout=60):
        lines = []
        end = time.time() + timeout
        while time.time() < end:
            line = self.process.stdout.readline()
            if not line:
                break
            line = line.strip()
            lines.append(line)
            if line.startswith(prefix):
                return lines
        raise RuntimeError(f"timeout waiting for {prefix}: {lines[-5:]}")

    def start(self, tb_path):
        self.send("uci")
        self.read_until("uciok")
        self.send(f"setoption name SyzygyPath value {tb_path}")
        self.send("setoption name SyzygyRootProbe value true")
        self.send("isready")
        self.read_until("readyok")

    def probe(self, fen):
        """Returns (move, wdl, dtz) from the root probe line, or None."""
        self.send(f"position fen {fen}")
        self.send("go movetime 50")
        lines = self.read_until("bestmove")
        for line in lines:
            if line.startswith("info string tablebase move:"):
                # info string tablebase move: d2e2 (wdl 2, dtz 5)
                body = line.split(":", 1)[1].strip()
                move = body.split()[0]
                wdl = int(body.split("wdl")[1].split(",")[0])
                dtz = int(body.split("dtz")[1].strip(" )"))
                return move, wdl, dtz
        return None

    def close(self):
        try:
            self.send("quit")
            self.process.wait(timeout=10)
        except Exception:
            self.process.kill()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", required=True, help="engine under test")
    parser.add_argument("--other", help="second engine (must agree on the move)")
    parser.add_argument(
        "--tb-path",
        required=True,
        action="append",
        help="table directory (repeat for several, e.g. 3-4-5 and 6-man)",
    )
    parser.add_argument("--samples", type=int, default=1000)
    parser.add_argument("--seed", type=int, default=20261119)
    args = parser.parse_args()

    rng = random.Random(args.seed)
    names = sorted({n for path in args.tb_path for n in material_names(path)})
    if not names:
        print("no table pairs found", file=sys.stderr)
        return 2

    reader = chess.syzygy.open_tablebase(args.tb_path[0])
    for extra in args.tb_path[1:]:
        reader.add_directory(extra)
    joined = os.pathsep.join(args.tb_path)
    engines = [Engine(args.engine)]
    if args.other:
        engines.append(Engine(args.other))
    for engine in engines:
        engine.start(joined)

    checked = skipped = 0
    failures = []
    by_pieces = {}
    while checked < args.samples:
        name = rng.choice(names)
        board = random_position(name, rng)
        if board is None:
            skipped += 1
            continue
        fen = board.fen(en_passant="fen")
        try:
            want_wdl = reader.probe_wdl(board)
            want_dtz = abs(reader.probe_dtz(board))
        except Exception as error:  # table missing in the reader
            skipped += 1
            continue

        answers = [engine.probe(fen) for engine in engines]
        main = answers[0]
        if main is None:
            failures.append((fen, "engine gave no tablebase answer", answers))
        else:
            move, wdl, dtz = main
            if wdl != want_wdl or dtz != want_dtz:
                failures.append(
                    (fen, f"engine wdl/dtz {wdl}/{dtz} != reader {want_wdl}/{want_dtz}", answers)
                )
            elif chess.Move.from_uci(move) not in board.legal_moves:
                failures.append((fen, f"illegal tablebase move {move}", answers))
            if len(answers) > 1 and answers[1] != main:
                failures.append((fen, "engines disagree", answers))
        checked += 1
        count = bin(board.occupied).count("1")
        by_pieces[count] = by_pieces.get(count, 0) + 1

    for engine in engines:
        engine.close()

    print(f"checked {checked} positions ({skipped} skipped), by piece count: {dict(sorted(by_pieces.items()))}")
    if failures:
        print(f"{len(failures)} FAILURES (first 10):")
        for fen, reason, answers in failures[:10]:
            print(f"  {fen}: {reason} {answers}")
        return 1
    print("all positions agree")
    return 0


if __name__ == "__main__":
    sys.exit(main())
