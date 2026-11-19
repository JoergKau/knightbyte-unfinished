#!/usr/bin/env python3
"""Generate docs/LEVERS.md from src/search/lever_table.rs.

The lever table is the single source of truth: this script only formats it, so
the document cannot drift from the engine. Run it after changing the table:

    python tools/levers_doc.py

`tests` check the table's invariants; `tests/lever_effect.rs` proves that every
lever changes the search.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TABLE = ROOT / "src" / "search" / "lever_table.rs"
OUT = ROOT / "docs" / "LEVERS.md"

# Levers that exist in the reference's catalogue (same name, same meaning).
REFERENCE = {
    "AspirationBase", "AspirationStartDepth", "AspirationMaxCycles",
    "UseStableTimeManagement", "NmpBaseReduction", "NmpDepthDivisor",
    "RfpFormulaMaxDepth", "RfpImprovingScale", "RfpNotImprovingScale",
    "RazorMaxDepth", "FutilityMaxDepth", "LmpMaxDepth", "HistPruneMargin",
    "SeePruningMaxDepth", "UseNegativeExtensions", "UseTrioHindsight",
    "UseTerm1NotImproving", "UseCutNode", "UseImprovingFlag", "QsDeltaMargin",
    "UseQsNarrowing", "UseMicroRiders", "UseQsTtProbe", "UseTtScoreEval",
    "QsRecaptureOnlyPly", "QsPromotionMaxPly", "UseQsCorrectionHistory",
    "UseProbCut", "UseTtExtension", "TtExtensionMaxDepth", "UseBadNoisyPruning",
}
KNIGHTBYTE_SWITCH = {
    "UseCorrectionHistory", "UseCheckAwareQuiescence", "UseSeeApproxPrefilter",
}
PORTED = {
    "UseProbCut", "ProbCutMargin", "ProbCutMinDepth", "UseTtExtension",
    "TtExtensionMaxDepth", "UseBadNoisyPruning", "BnpDepth",
    "UseQsCorrectionHistory",
}

# The predecessor's v9 campaign (400 games each, 10+0.1, one thread, error bar
# about +/-22 Elo, every arm formally inconclusive). Elo of the arm named.
PRIOR = {
    "UseStableTimeManagement": "off: -24.4",
    "NmpDepthDivisor": "=2: -27.0",
    "UseImprovingFlag": "off: -10.4",
    "UseMicroRiders": "off: -9.6",
    "UseTtScoreEval": "off: -8.7",
    "AspirationBase": "=33: +6.1",
    "UseCutNode": "off: +2.6",
    "UseNegativeExtensions": "off: 0.0",
    "UseQsTtProbe": "off: -0.9",
    "UseQsNarrowing": "on: +5.2",
    "UseTtExtension": "on: +9.6",
    "UseQsCorrectionHistory": "on: +8.7",
    "UseBadNoisyPruning": "on: +4.8 (800 games)",
    "UseProbCut": "on, with its secondary layer: -4.3",
    "SeePruningMaxDepth": "SEE thresholds + depth 8 off: -8.7",
}

GROUP_DESCRIPTIONS = {}


def parse():
    text = TABLE.read_text(encoding="utf8")
    levers = []
    group = None
    # One scan over the table: a group header (a comment between two rules)
    # sets the current group, a `LeverSpec` block is a lever.
    pattern = re.compile(
        r"    // -{10,}\n    // ([^\n]+)\n    // -{10,}|"
        r"    LeverSpec \{\n"
        r'        name: "([^"]+)",\n'
        r"        kind: OptionKind::(\w+),\n"
        r"        default: (-?[\d_]+),\n"
        r"        min: (-?[\d_]+),\n"
        r"        max: (-?[\d_]+),\n"
        r'        help: "((?:[^"\\]|\\.)*)",\n'
        r"    \},"
    )
    for match in pattern.finditer(text):
        if match.group(1):
            group = match.group(1)
            continue
        name, kind, default, lo, hi, help_text = match.groups()[1:]
        levers.append({
            "group": group,
            "name": name,
            "kind": "check" if kind == "Check" else "spin",
            "default": default.replace("_", ""),
            "min": lo.replace("_", ""),
            "max": hi.replace("_", ""),
            "help": help_text.replace('\\"', '"'),
        })
    return levers


def origin(name):
    if name in PORTED:
        return "ported, ships off"
    if name in KNIGHTBYTE_SWITCH:
        return "KnightByte switch"
    if name in REFERENCE:
        return "reference"
    return "constant, exposed"


def main():
    levers = parse()
    if len(levers) != 60:
        sys.exit(f"expected 60 levers, found {len(levers)}")
    lines = [
        "# KnightByte research levers",
        "",
        "Generated from `src/search/lever_table.rs` by `tools/levers_doc.py`; do not",
        "edit by hand. There are exactly **sixty** levers, all functional (decision:",
        "`docs/decisions/ADR-0014-the-sixty-levers.md`). Each one is a UCI option:",
        "`setoption name <Name> value <value>`. A check lever takes `true` or `false`.",
        "",
        "`tests/lever_effect.rs` sets every lever to a non-default value and requires the",
        "search to change (run it with `cargo test --release --test lever_effect`);",
        "`UseStableTimeManagement` only acts on a clock and is covered by a time-manager",
        "test. Defaults reproduce the pinned search: `bench 6/12/16/18` =",
        "15,528 / 456,308 / 2,815,799 / 6,892,874.",
        "",
        "**Origin:** *reference* = a lever of the predecessor's catalogue; *constant,",
        "exposed* = a value that was hard-coded in the search, now a lever whose default",
        "is that value; *ported, ships off* = a feature added with a switch that is off",
        "by default; *KnightByte switch* = switches off a feature the reference always",
        "runs.",
        "",
        "**Prior evidence** is the predecessor's v9 campaign: 400 games per arm (10+0.1,",
        "one thread, UHO), error bar about +/-22 Elo, every arm formally inconclusive.",
        "It is a hint about direction, not a result, and it was measured on a different",
        "engine and net.",
        "",
    ]
    current = None
    number = 0
    for lever in levers:
        if lever["group"] != current:
            current = lever["group"]
            lines += ["", f"## {current}", "",
                      "| # | Lever | Type | Default | Range | Origin | Prior evidence | What it does |",
                      "| ---: | :--- | :--- | ---: | :--- | :--- | :--- | :--- |"]
        number += 1
        if lever["kind"] == "check":
            default = "true" if lever["default"] != "0" else "false"
            span = "true / false"
        else:
            default = lever["default"]
            span = f'{lever["min"]} to {lever["max"]}'
        lines.append(
            f'| {number} | `{lever["name"]}` | {lever["kind"]} | {default} | {span} | '
            f'{origin(lever["name"])} | {PRIOR.get(lever["name"], "")} | {lever["help"]} |'
        )
    lines.append("")
    OUT.write_text("\n".join(lines), encoding="utf8", newline="\n")
    print(f"wrote {OUT} ({number} levers)")


if __name__ == "__main__":
    main()
