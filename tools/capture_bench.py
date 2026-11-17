#!/usr/bin/env python3
"""Capture and compare fixed-depth `bench` node counts.

The search-parity gate compares per-position node counts at fixed depth. This
tool runs any UCI engine's `bench <depth>` command, parses the per-position and
total node counts, and stores them as JSON so a baseline can be re-checked later
without the original engine.

Usage:
    # record a baseline (engine path is never written into the file)
    python tools/capture_bench.py capture --engine ENGINE --label reference \
        --depths 1-12,14,16,18 --out tests/golden/search_parity.json

    # compare a build against a recorded baseline
    python tools/capture_bench.py compare --engine ENGINE \
        --baseline tests/golden/search_parity.json --depths 1-12

Both engine output shapes are understood:

    info string bench position 1 nodes 9480 bestmove d2d4   (KnightByte)
    Position 1/4: ... Nodes: 3.202 ...                        (reference)

Node counts are parsed by stripping every non-digit, because the reference
formats numbers with the host's locale separators.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys

POSITION_INFO = re.compile(r"info string bench position (\d+) nodes (\d+)")
POSITION_BANNER = re.compile(r"Position (\d+)/(\d+):")
NODE_LINE = re.compile(r"^Nodes:\s*([0-9.,\s]+)$")
TOTAL_LINE = re.compile(r"^Total Nodes:\s*([0-9.,\s]+)$")


def parse_depths(text: str) -> list[int]:
    """Expands "1-12,16,18" into [1..12, 16, 18]."""
    depths: list[int] = []
    for part in text.split(","):
        part = part.strip()
        if not part:
            continue
        if "-" in part:
            first, last = part.split("-", 1)
            depths.extend(range(int(first), int(last) + 1))
        else:
            depths.append(int(part))
    return sorted(set(depths))


def as_number(text: str) -> int:
    """Returns the integer value of a locale-formatted count."""
    digits = re.sub(r"\D", "", text)
    return int(digits) if digits else 0


def run_bench(engine: str, depth: int, timeout: int) -> str:
    """Runs `bench <depth>` and returns the captured stdout."""
    completed = subprocess.run(
        [engine],
        input=f"bench {depth}\nquit\n",
        capture_output=True,
        text=True,
        timeout=timeout,
        encoding="utf-8",
        errors="replace",
    )
    return completed.stdout


def parse_bench(output: str) -> tuple[list[int], int] | None:
    """Extracts per-position node counts and the total from a bench transcript."""
    positions: dict[int, int] = {}
    current: int | None = None
    total = 0
    saw_reference_total = False

    for raw in output.splitlines():
        line = raw.strip()

        info = POSITION_INFO.search(line)
        if info:
            positions[int(info.group(1))] = int(info.group(2))
            continue

        banner = POSITION_BANNER.search(line)
        if banner:
            current = int(banner.group(1))
            continue

        total_match = TOTAL_LINE.match(line)
        if total_match:
            total = as_number(total_match.group(1))
            saw_reference_total = True
            continue

        node_match = NODE_LINE.match(line)
        if node_match and current is not None:
            positions[current] = as_number(node_match.group(1))

    if not positions:
        return None

    ordered = [positions[index] for index in sorted(positions)]
    if not saw_reference_total:
        total = sum(ordered)
    return ordered, total


def capture(engine: str, label: str, depths: list[int], timeout: int) -> dict:
    """Runs every depth and returns the baseline document."""
    result: dict = {"label": label, "depths": {}}
    for depth in depths:
        parsed = parse_bench(run_bench(engine, depth, timeout))
        if parsed is None:
            print(f"error: could not parse bench {depth}", file=sys.stderr)
            continue
        positions, total = parsed
        if sum(positions) != total:
            print(
                f"warning: depth {depth}: per-position sum {sum(positions)} "
                f"differs from reported total {total}",
                file=sys.stderr,
            )
        result["depths"][str(depth)] = {"positions": positions, "total": total}
        print(f"depth {depth:>2}: {positions} total {total}", file=sys.stderr)
    return result


def compare(engine: str, baseline: dict, depths: list[int] | None, timeout: int) -> bool:
    """Compares an engine against a baseline; returns True when everything matches."""
    wanted = [str(depth) for depth in depths] if depths else list(baseline["depths"])
    matches = True
    print(f"{'depth':>5} {'pos1':>9} {'pos2':>9} {'pos3':>9} {'pos4':>9} {'total delta':>12}")
    for key in wanted:
        if key not in baseline["depths"]:
            print(f"warning: baseline has no depth {key}", file=sys.stderr)
            continue
        parsed = parse_bench(run_bench(engine, int(key), timeout))
        if parsed is None:
            print(f"error: could not parse bench {key}", file=sys.stderr)
            matches = False
            continue
        positions, total = parsed
        expected = baseline["depths"][key]
        deltas = [
            observed - reference
            for observed, reference in zip(positions, expected["positions"])
        ]
        total_delta = total - expected["total"]
        if any(delta != 0 for delta in deltas) or total_delta != 0:
            matches = False
        print(
            f"{key:>5} "
            + " ".join(f"{delta:>+9}" for delta in deltas)
            + f" {total_delta:>+12}"
        )
    return matches


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    capture_parser = sub.add_parser("capture", help="record a baseline")
    capture_parser.add_argument("--engine", required=True)
    capture_parser.add_argument("--label", default="unlabelled")
    capture_parser.add_argument("--depths", default="1-12,16,18")
    capture_parser.add_argument("--out", required=True)
    capture_parser.add_argument("--timeout", type=int, default=600)

    compare_parser = sub.add_parser("compare", help="compare against a baseline")
    compare_parser.add_argument("--engine", required=True)
    compare_parser.add_argument("--baseline", required=True)
    compare_parser.add_argument("--depths", default=None)
    compare_parser.add_argument("--timeout", type=int, default=600)

    args = parser.parse_args(argv)

    if args.command == "capture":
        document = capture(
            args.engine, args.label, parse_depths(args.depths), args.timeout
        )
        with open(args.out, "w", encoding="utf-8", newline="\n") as handle:
            json.dump(document, handle, indent=2)
            handle.write("\n")
        print(f"wrote {args.out}", file=sys.stderr)
        return 0

    with open(args.baseline, encoding="utf-8") as handle:
        baseline = json.load(handle)
    depths = parse_depths(args.depths) if args.depths else None
    matches = compare(args.engine, baseline, depths, args.timeout)
    return 0 if matches else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
