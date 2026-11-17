#!/usr/bin/env python3
"""Extract the research-lever catalog from a reference implementation source.

The predecessor registers tuning levers through ``AddLever(new UciOption(...))``
with either a numeric or a boolean default. This script produces the
machine-readable catalog KnightByte must expose; it does not copy source code.

Usage:
    python tools/capture_levers.py --source /path/to/EngineOrchestrator.cs \
        --out tests/golden/levers.json
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

LEVER_RE = re.compile(
    r"AddLever\(\s*new\s+UciOption\(\s*nameof\(\s*SearchEngine\.(?P<name>\w+)\s*\)"
    r"\s*,\s*(?P<default>[^,)]+)"
    r"(?:\s*,\s*(?P<min>-?\d+)\s*,\s*(?P<max>-?\d+))?"
)


def classify(default: str, minimum: str | None, maximum: str | None) -> dict:
    """Infers the UCI option type from the constructor overload."""
    value = default.strip()
    if value in ("true", "false"):
        return {
            "name": "",
            "type": "check",
            "default": value,
            "min": None,
            "max": None,
        }
    if re.fullmatch(r"-?\d+", value):
        return {
            "name": "",
            "type": "spin",
            "default": value,
            "min": int(minimum) if minimum else None,
            "max": int(maximum) if maximum else None,
        }
    return {
        "name": "",
        "type": "string",
        "default": value.strip('"'),
        "min": None,
        "max": None,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, help="reference source file")
    parser.add_argument(
        "--out",
        default="tests/golden/levers.json",
        help="output JSON path (default: tests/golden/levers.json)",
    )
    args = parser.parse_args()

    source = Path(args.source)
    if not source.is_file():
        print(f"error: source not found: {source}", file=sys.stderr)
        return 1

    text = source.read_text(encoding="utf-8")
    levers: list[dict] = []
    seen: set[str] = set()
    for match in LEVER_RE.finditer(text):
        name = match.group("name")
        if name in seen:
            continue
        seen.add(name)
        record = classify(
            match.group("default"), match.group("min"), match.group("max")
        )
        record["name"] = name
        levers.append(record)

    levers.sort(key=lambda item: item["name"])
    path = Path(args.out)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        json.dump(levers, handle, indent=2)
        handle.write("\n")
    print(f"wrote {path} ({len(levers)} levers)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
