#!/usr/bin/env python3
"""Repeat the deterministic bench and report the NPS distribution.

The engine's own ``bench`` command prints ``Nodes:`` and ``NPS:``. This harness
runs it in a fresh process several times, checks the node identity pin for the
depth, and reports the median/min/max NPS. NPS is host-relative: compare runs
from the same host and the same build class only.

Usage:
    python tools/perf_bench.py --engine ./target/release/knightbyte \
        --depth 12 --runs 5 --json perf.json

Exit codes:
    0  all runs parsed and the node count matched the pin
    1  an engine run failed or printed no result
    2  the node count did not match the pin (unless --allow-drift)
"""

import argparse
import json
import platform
import re
import statistics
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

# Node identity pins for the four-position bench (single thread, 256 MB hash,
# cleared table per position). They must match exactly; a drift means the
# search changed, not that the machine is fast or slow.
NODE_PINS = {
    6: 15_528,
    12: 456_308,
    16: 2_815_799,
    18: 6_892_874,
}

NODES_RE = re.compile(r"^Nodes:\s*([\d.,]+)\s*$", re.MULTILINE)
NPS_RE = re.compile(r"^NPS:\s*([\d.,]+)\s*$", re.MULTILINE)
# Some engines print a benchmark summary with dot/comma thousands separators
# ("Total Nodes: 456.308", "Average NPS: 708.552"). Prefer those when present.
TOTAL_NODES_RE = re.compile(r"^Total Nodes:\s*([\d.,]+)\s*$", re.MULTILINE | re.IGNORECASE)
AVG_NPS_RE = re.compile(r"^Average NPS:\s*([\d.,]+)\s*$", re.MULTILINE | re.IGNORECASE)


def parse_count(text):
    """Parses a count that may use dot or comma thousands separators."""
    return int(text.replace(".", "").replace(",", ""))


def git_metadata(repo):
    """Return ``{"commit": ..., "dirty": ...}`` for the given directory."""
    def run(*args):
        try:
            result = subprocess.run(
                ["git", "-C", str(repo), *args],
                capture_output=True,
                text=True,
                timeout=10,
                check=False,
            )
        except (OSError, subprocess.SubprocessError):
            return None
        if result.returncode != 0:
            return None
        return result.stdout.strip()

    commit = run("rev-parse", "--short", "HEAD")
    status = run("status", "--porcelain")
    return {
        "commit": commit or "unknown",
        "dirty": None if status is None else bool(status),
    }


def run_bench(engine, depth, timeout):
    """Run one fresh process and return ``(nodes, nps)``.

    Commands go through stdin so the harness works with a binary that only
    speaks the UCI loop as well as with one that accepts a one-shot command
    line.
    """
    process = subprocess.run(
        [str(engine)],
        input="bench {}\nquit\n".format(depth),
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
    )
    nodes_match = TOTAL_NODES_RE.search(process.stdout) or NODES_RE.search(process.stdout)
    nps_match = AVG_NPS_RE.search(process.stdout) or NPS_RE.search(process.stdout)
    if nodes_match is None or nps_match is None:
        raise RuntimeError(
            "bench produced no result (exit {}):\n{}".format(
                process.returncode, process.stdout[-2000:] or process.stderr[-2000:]
            )
        )
    return parse_count(nodes_match.group(1)), parse_count(nps_match.group(1))


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--engine", required=True, help="path to the engine binary")
    parser.add_argument("--depth", type=int, default=12, help="bench depth (default 12)")
    parser.add_argument("--runs", type=int, default=5, help="measured runs (default 5)")
    parser.add_argument(
        "--warmup", type=int, default=1, help="discarded warmup runs (default 1)"
    )
    parser.add_argument(
        "--expect-nodes",
        type=int,
        default=None,
        help="override the expected node count (default: the pin for --depth)",
    )
    parser.add_argument(
        "--allow-drift",
        action="store_true",
        help="report a node drift without failing (never use for a parity gate)",
    )
    parser.add_argument("--timeout", type=float, default=300.0, help="seconds per run")
    parser.add_argument("--json", default=None, help="write a JSON record to this path")
    parser.add_argument(
        "--repo",
        default=".",
        help="directory used for the git commit/dirty metadata (default: cwd)",
    )
    args = parser.parse_args()

    engine = Path(args.engine).resolve()
    if not engine.is_file():
        print("engine not found: {}".format(engine), file=sys.stderr)
        return 1
    if args.runs < 1:
        print("--runs must be at least 1", file=sys.stderr)
        return 1

    expected = (
        args.expect_nodes
        if args.expect_nodes is not None
        else NODE_PINS.get(args.depth)
    )

    try:
        for index in range(args.warmup):
            print("warmup {}/{} ...".format(index + 1, args.warmup))
            run_bench(engine, args.depth, args.timeout)

        nodes = []
        nps = []
        for index in range(args.runs):
            run_nodes, run_nps = run_bench(engine, args.depth, args.timeout)
            nodes.append(run_nodes)
            nps.append(run_nps)
            print("run {}: nodes {} nps {}".format(index + 1, run_nodes, run_nps))
    except (OSError, subprocess.SubprocessError, RuntimeError) as error:
        print("bench failed: {}".format(error), file=sys.stderr)
        return 1

    node_ok = expected is None or all(value == expected for value in nodes)
    record = {
        "timestamp_utc": datetime.now(timezone.utc).isoformat(),
        "engine": engine.name,
        "depth": args.depth,
        "runs": args.runs,
        "warmup": args.warmup,
        "nodes": nodes,
        "nps": nps,
        "nodes_median": int(statistics.median(nodes)),
        "nps_median": int(statistics.median(nps)),
        "nps_min": min(nps),
        "nps_max": max(nps),
        "expected_nodes": expected,
        "node_identity_ok": node_ok,
        "host": {
            "node": platform.node(),
            "machine": platform.machine(),
            "processor": platform.processor(),
            "platform": platform.platform(),
        },
        "git": git_metadata(args.repo),
    }

    print()
    print("median NPS: {:,} (min {:,}, max {:,})".format(
        record["nps_median"], record["nps_min"], record["nps_max"]
    ))
    if expected is None:
        print("node pin: not defined for depth {}".format(args.depth))
    else:
        print(
            "node pin: {} (expected {:,})".format(
                "OK" if node_ok else "DRIFT", expected
            )
        )

    if args.json:
        Path(args.json).write_text(json.dumps(record, indent=2), encoding="utf-8")
        print("record written to {}".format(args.json))

    if not node_ok and not args.allow_drift:
        print(
            "node count moved: this is a search change, not a speed result",
            file=sys.stderr,
        )
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
