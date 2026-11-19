#!/usr/bin/env python3
"""Differential opening-book check between two engines.

Walks random lines through a ``.ctg`` book. At every position both engines are
asked repeatedly for a book move (book moves are chosen by weighted random
choice, so a single sample is not enough); the *sets* of moves each engine can
return, and whether a position is in the book at all, must agree.

Usage::

    python tools/book_compare.py --engine target/release/knightbyte.exe \
        --other other.exe --book books/some.ctg --lines 30 --plies 12
"""

import argparse
import random
import subprocess
import sys
import time


class Engine:
    """A minimal UCI session with the book enabled."""

    def __init__(self, path, book):
        self.process = subprocess.Popen(
            [path],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        self.send("uci")
        self.read_until("uciok")
        self.send(f"setoption name Book File value {book}")
        self.send("setoption name Own Book value true")
        self.send("setoption name Book Depth value 100")
        self.send("isready")
        self.read_until("readyok")

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
        raise RuntimeError(f"timeout waiting for {prefix}")

    def book_move(self, moves):
        """Returns the book move for the line, or None when out of book."""
        self.send("position startpos" + (" moves " + " ".join(moves) if moves else ""))
        self.send("go depth 1")
        lines = self.read_until("bestmove")
        if any(line.startswith("info string book move:") for line in lines):
            return lines[-1].split()[1]
        return None

    def close(self):
        try:
            self.send("quit")
            self.process.wait(timeout=10)
        except Exception:
            self.process.kill()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine", required=True)
    parser.add_argument("--other", required=True)
    parser.add_argument("--book", required=True)
    parser.add_argument("--lines", type=int, default=30)
    parser.add_argument("--plies", type=int, default=12)
    parser.add_argument("--samples", type=int, default=80)
    parser.add_argument("--seed", type=int, default=20261119)
    args = parser.parse_args()

    rng = random.Random(args.seed)
    ours = Engine(args.engine, args.book)
    theirs = Engine(args.other, args.book)

    positions = disagreements = in_book = 0
    seen = set()
    for _ in range(args.lines):
        moves = []
        for _ply in range(args.plies):
            key = tuple(moves)
            if key not in seen:
                seen.add(key)
                a = {ours.book_move(moves) for _ in range(args.samples)}
                b = {theirs.book_move(moves) for _ in range(args.samples)}
                positions += 1
                if a != b:
                    disagreements += 1
                    print(f"DISAGREE after {' '.join(moves) or '(start)'}: "
                          f"ours={sorted(m or '-' for m in a)} other={sorted(m or '-' for m in b)}")
                if a != {None}:
                    in_book += 1
                choices = sorted(m for m in (a | b) if m)
            else:
                choices = sorted(m for m in {ours.book_move(moves) for _ in range(20)} if m)
            if not choices:
                break
            moves.append(rng.choice(choices))

    ours.close()
    theirs.close()
    print(f"{positions} positions compared ({in_book} in book), {disagreements} disagreements")
    return 1 if disagreements else 0


if __name__ == "__main__":
    sys.exit(main())
