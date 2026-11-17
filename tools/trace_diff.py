#!/usr/bin/env python3
"""Diff two parity-trace ledgers.

The parity hunt writes one ledger line per search node from both engines
(`KNIGHTBYTE_TRACE=1` for KnightByte, `PF_TRACE=1` for the instrumented local
reference build) and diffs them line by line. Node numbers are sequential in
both engines, so the first line whose fields disagree is the first divergent
node — the one whose deciding formula must be read in the reference source.

Usage:
    python tools/trace_diff.py knightbyte.trace reference.trace
"""
from __future__ import annotations

import re
import sys

FIELD = re.compile(
    r"^T\s+(?P<kind>\w)\s+n\s+(?P<node>\d+)\s+p\s+(?P<ply>\d+)\s+"
    r"d\s+(?P<depth>-?\d+)\s+a\s+(?P<alpha>-?\d+)\s+b\s+(?P<beta>-?\d+)\s+"
    r"c\s+(?P<cut>\d)\s+s\s+(?P<score>-?\d+)\s+m\s*(?P<path>.*)$"
)


def normalize(line: str) -> tuple | None:
    """Parses one ledger line into its comparable fields."""
    match = FIELD.match(line.strip())
    if match is None:
        return None
    fields = match.groupdict()
    # A full window in one engine is the i32 extremes, in the other ±32000;
    # both mean "unbounded", so they compare equal.
    def window(value: str) -> str:
        number = int(value)
        if number <= -30_000:
            return "-W"
        if number >= 30_000:
            return "W"
        return value

    # Return lines have engine-specific path formatting; they compare by
    # their scored fields only. The reference's null move is a history record
    # whose Move.None prints as "a1a1"; KnightByte's is filtered out entirely.
    # Drop the token so both ledgers describe the same node path.
    path = fields["path"].replace("a1a1", "").strip()
    if fields["kind"] == "r":
        path = ""

    return (
        fields["kind"],
        int(fields["node"]),
        int(fields["ply"]),
        int(fields["depth"]),
        window(fields["alpha"]),
        window(fields["beta"]),
        fields["cut"],
        path,
    )


def load(path: str) -> list[tuple]:
    with open(path, encoding="utf-8", errors="replace") as handle:
        entries = [normalize(line) for line in handle]
    return [entry for entry in entries if entry is not None]


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print(__doc__)
        return 2
    ours = load(argv[1])
    theirs = load(argv[2])
    print(f"knightbyte nodes: {len(ours)}, reference nodes: {len(theirs)}")
    for index, (a, b) in enumerate(zip(ours, theirs)):
        if a != b:
            print(f"first divergence at ledger line {index + 1}:")
            print(f"  knightbyte: {a}")
            print(f"  reference:  {b}")
            context_start = max(0, index - 3)
            for back in range(context_start, index):
                print(f"    ctx {back + 1}: {ours[back]}")
            return 1
    if len(ours) != len(theirs):
        print(
            f"ledgers agree on the first {min(len(ours), len(theirs))} nodes; "
            f"one engine searched {abs(len(ours) - len(theirs))} more"
        )
        longer, name = (ours, "knightbyte") if len(ours) > len(theirs) else (theirs, "reference")
        for entry in longer[min(len(ours), len(theirs)) :][:5]:
            print(f"  extra {name}: {entry}")
        return 1
    print("ledgers identical")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
