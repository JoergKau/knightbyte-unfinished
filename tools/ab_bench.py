#!/usr/bin/env python3
"""Balanced A/B speed comparison of two engine binaries on one host.

A single back-to-back pair is biased: on this class of host the process that
runs first in a pair measured 2-4 % faster even when both binaries were
byte-identical (boost state, caches). This harness removes the bias by
construction. Each round runs the engines in the order A B B A, so every
engine is first once and last once, and the reported change is the ratio of
the mean NPS over all rounds. Run it with the same binary twice (an A/A
control) to see the noise floor of the day before believing a small gain.

Every run also checks the node pin for the depth, so a "faster" binary that
searches a different tree is rejected, not reported.

Usage:
    python tools/ab_bench.py A.exe B.exe --depth 16 --rounds 4
    python tools/ab_bench.py A.exe A.exe --depth 16 --rounds 4   # A/A control

Exit codes: 0 done, 1 a run failed, 2 a node count did not match the pin.
"""

import argparse
import statistics
import sys

from perf_bench import NODE_PINS, run_bench


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("engine_a")
    parser.add_argument("engine_b")
    parser.add_argument("--depth", type=int, default=16)
    parser.add_argument("--rounds", type=int, default=4, help="ABBA rounds (default 4)")
    parser.add_argument("--timeout", type=int, default=600)
    args = parser.parse_args()

    pin = NODE_PINS.get(args.depth)
    samples = {"A": [], "B": []}
    engines = {"A": args.engine_a, "B": args.engine_b}

    # One discarded warm-up per engine (page cache, first-touch).
    for name in ("A", "B"):
        run_bench(engines[name], args.depth, args.timeout)

    for round_index in range(args.rounds):
        for name in ("A", "B", "B", "A"):
            nodes, nps = run_bench(engines[name], args.depth, args.timeout)
            if pin is not None and nodes != pin:
                print(f"node pin mismatch for {name}: {nodes} != {pin}", file=sys.stderr)
                return 2
            samples[name].append(nps)
        print(
            f"round {round_index + 1}: A {statistics.mean(samples['A'][-2:]):,.0f}"
            f"  B {statistics.mean(samples['B'][-2:]):,.0f}"
        )

    mean_a = statistics.mean(samples["A"])
    mean_b = statistics.mean(samples["B"])
    spread_a = statistics.pstdev(samples["A"]) / mean_a * 100
    spread_b = statistics.pstdev(samples["B"]) / mean_b * 100
    print(f"A mean {mean_a:,.0f} NPS (sd {spread_a:.1f} %, {len(samples['A'])} runs)")
    print(f"B mean {mean_b:,.0f} NPS (sd {spread_b:.1f} %, {len(samples['B'])} runs)")
    print(f"B / A = {mean_b / mean_a:.4f}  ({(mean_b / mean_a - 1) * 100:+.2f} %)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
