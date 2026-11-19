#!/usr/bin/env python3
"""Run the lever test queue of docs/audit-tests-KB.md, one arm after the other.

Generated together with the document; the arm table below is the same data as
its queue tables (id, option string, base options, games, and the fixed-depth
pre-flight totals measured on the frozen commit).

    python tools/lever_campaign.py --engine <the campaign binary> --group P1
    python tools/lever_campaign.py --only P1.03        # one arm
    python tools/lever_campaign.py --all               # the whole queue (about 34 hours)

(KB_ENGINE can replace --engine.) Before each arm the script runs the
pre-flight: `bench 12` with the arm must print the recorded total, and the base
alone must print its recorded total. A mismatch means a different binary or a
lever that is not wired, and the arm is not played. An arm whose
matches/<file>.result.txt exists is skipped (resume), unless --force is given.
Nothing is baked: results go to matches/ and are copied into the results log of
the document by hand.

--games and --tc override the standard (400 games, 10+0.1) for a smoke test.
"""

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import sprt_arm  # noqa: E402

# The frozen default total: every pre-flight below is relative to it.
DEFAULT_BENCH = 456308

ARMS = [
    {"id": "P1.01", "file": "P1_01_UseProbCut", "arm": "UseProbCut=true", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 442190},
    {"id": "P1.02", "file": "P1_02_UseTtExtension", "arm": "UseTtExtension=true", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 502408},
    {"id": "P1.03", "file": "P1_03_UseQsCorrectionHistory", "arm": "UseQsCorrectionHistory=true", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 536219},
    {"id": "P1.04", "file": "P1_04_UseBadNoisyPruning", "arm": "UseBadNoisyPruning=true", "base": "", "games": 800, "base_bench": 456308, "arm_bench": 445981},
    {"id": "P1.05", "file": "P1_05_LmrDivisor", "arm": "LmrDivisor=225", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 573915},
    {"id": "P1.06", "file": "P1_06_LmrHistoryDivisor", "arm": "LmrHistoryDivisor=2500", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 442829},
    {"id": "P1.07", "file": "P1_07_LmrPvWeight", "arm": "LmrPvWeight=600", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 413497},
    {"id": "P1.08", "file": "P1_08_LmrKillerWeight", "arm": "LmrKillerWeight=600", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 518382},
    {"id": "P1.09", "file": "P1_09_LmrMinMoves", "arm": "LmrMinMoves=2", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 370559},
    {"id": "P1.10", "file": "P1_10_UseTerm1NotImproving", "arm": "UseTerm1NotImproving=true", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 330643},
    {"id": "P1.11", "file": "P1_11_NmpBaseReduction", "arm": "NmpBaseReduction=4", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 380463},
    {"id": "P1.12", "file": "P1_12_NmpEvalDivisor", "arm": "NmpEvalDivisor=140", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 388793},
    {"id": "P1.13", "file": "P1_13_RfpNotImprovingScale", "arm": "RfpNotImprovingScale=80", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 440620},
    {"id": "P1.14", "file": "P1_14_FutilityScale", "arm": "FutilityScale=85", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 532647},
    {"id": "P1.15", "file": "P1_15_LmpScale", "arm": "LmpScale=80", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 442123},
    {"id": "P1.16", "file": "P1_16_UseQsNarrowing", "arm": "UseQsNarrowing=true", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 428894},
    {"id": "P2.01", "file": "P2_01_UseTtScoreEval", "arm": "UseTtScoreEval=false", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 388414},
    {"id": "P2.02", "file": "P2_02_UseMicroRiders", "arm": "UseMicroRiders=false", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 437184},
    {"id": "P2.03", "file": "P2_03_UseImprovingFlag", "arm": "UseImprovingFlag=false", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 603732},
    {"id": "P2.04", "file": "P2_04_UseCutNode", "arm": "UseCutNode=false", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 436287},
    {"id": "P2.05", "file": "P2_05_UseNegativeExtensions", "arm": "UseNegativeExtensions=false", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 439126},
    {"id": "P2.06", "file": "P2_06_UseInternalIterativeReduction", "arm": "UseInternalIterativeReduction=false", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 480101},
    {"id": "P2.07", "file": "P2_07_UseTrioHindsight", "arm": "UseTrioHindsight=false", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 443994},
    {"id": "P2.08", "file": "P2_08_UseHistoryPruning", "arm": "UseHistoryPruning=false", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 537894},
    {"id": "P2.09", "file": "P2_09_UseReSearchReduction", "arm": "UseReSearchReduction=false", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 553573},
    {"id": "P3.01", "file": "P3_01_UseStableTimeManagement", "arm": "UseStableTimeManagement=false", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 456308},
    {"id": "P4.01", "file": "P4_01_UseCorrectionHistory", "arm": "UseCorrectionHistory=false", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 484853},
    {"id": "P4.02", "file": "P4_02_UseCheckAwareQuiescence", "arm": "UseCheckAwareQuiescence=false", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 422703},
    {"id": "P4.03", "file": "P4_03_UseSeeApproxPrefilter", "arm": "UseSeeApproxPrefilter=false", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 340948},
    {"id": "P4.04", "file": "P4_04_UseQsTtProbe", "arm": "UseQsTtProbe=false", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 449325},
    {"id": "P5.01", "file": "P5_01_AspirationBase", "arm": "AspirationBase=33", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 647408},
    {"id": "P5.02", "file": "P5_02_AspirationStartDepth", "arm": "AspirationStartDepth=7", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 368657},
    {"id": "P5.03", "file": "P5_03_NmpDepthDivisor", "arm": "NmpDepthDivisor=3", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 450679},
    {"id": "P5.04", "file": "P5_04_NmpEvalBonusMax", "arm": "NmpEvalBonusMax=0", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 444762},
    {"id": "P5.05", "file": "P5_05_NmpMinDepth", "arm": "NmpMinDepth=4", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 464233},
    {"id": "P5.06", "file": "P5_06_RfpFormulaMaxDepth", "arm": "RfpFormulaMaxDepth=10", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 455530},
    {"id": "P5.07", "file": "P5_07_RfpImprovingScale", "arm": "RfpImprovingScale=55", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 515700},
    {"id": "P5.08", "file": "P5_08_RfpDamping", "arm": "RfpDamping=2", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 438965},
    {"id": "P5.09", "file": "P5_09_RazorMaxDepth", "arm": "RazorMaxDepth=4", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 457019},
    {"id": "P5.10", "file": "P5_10_RazorMarginScale", "arm": "RazorMarginScale=80", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 563540},
    {"id": "P5.11", "file": "P5_11_FutilityMaxDepth", "arm": "FutilityMaxDepth=9", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 442770},
    {"id": "P5.12", "file": "P5_12_LmpMaxDepth", "arm": "LmpMaxDepth=6", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 438259},
    {"id": "P5.13", "file": "P5_13_HistPruneMaxDepth", "arm": "HistPruneMaxDepth=1", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 422845},
    {"id": "P5.14", "file": "P5_14_HistPruneMargin", "arm": "HistPruneMargin=1500", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 425556},
    {"id": "P5.15", "file": "P5_15_SeePruningMaxDepth", "arm": "SeePruningMaxDepth=10", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 451803},
    {"id": "P5.16", "file": "P5_16_SeeQuietMargin", "arm": "SeeQuietMargin=-30", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 406602},
    {"id": "P5.17", "file": "P5_17_SeeNoisyMargin", "arm": "SeeNoisyMargin=-18", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 467679},
    {"id": "P5.18", "file": "P5_18_SeeHistoryDivisor", "arm": "SeeHistoryDivisor=192", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 485273},
    {"id": "P5.19", "file": "P5_19_NegExtHistoryThreshold", "arm": "NegExtHistoryThreshold=2000", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 475746},
    {"id": "P5.20", "file": "P5_20_IirMinDepth", "arm": "IirMinDepth=6", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 392859},
    {"id": "P5.21", "file": "P5_21_HindsightMargin", "arm": "HindsightMargin=30", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 510289},
    {"id": "P5.22", "file": "P5_22_LmrMinDepth", "arm": "LmrMinDepth=2", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 460888},
    {"id": "P5.23", "file": "P5_23_QsDeltaMargin", "arm": "QsDeltaMargin=900", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 453686},
    {"id": "P5.24", "file": "P5_24_QsRecaptureOnlyPly", "arm": "QsRecaptureOnlyPly=4", "base": "UseQsNarrowing=true", "games": 400, "base_bench": 428894, "arm_bench": 437102},
    {"id": "P5.25", "file": "P5_25_HistoryBonusScale", "arm": "HistoryBonusScale=130", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 434036},
    {"id": "P5.26", "file": "P5_26_HistoryMalusScale", "arm": "HistoryMalusScale=130", "base": "", "games": 400, "base_bench": 456308, "arm_bench": 425788},
    {"id": "P5.27", "file": "P5_27_ProbCutMargin", "arm": "ProbCutMargin=180", "base": "UseProbCut=true", "games": 400, "base_bench": 442190, "arm_bench": 404945},
    {"id": "P5.28", "file": "P5_28_ProbCutMinDepth", "arm": "ProbCutMinDepth=7", "base": "UseProbCut=true", "games": 400, "base_bench": 442190, "arm_bench": 459622},
    {"id": "P5.29", "file": "P5_29_TtExtensionMaxDepth", "arm": "TtExtensionMaxDepth=8", "base": "UseTtExtension=true", "games": 400, "base_bench": 502408, "arm_bench": 538355},
    {"id": "P5.30", "file": "P5_30_BnpDepth", "arm": "BnpDepth=4", "base": "UseBadNoisyPruning=true", "games": 400, "base_bench": 445981, "arm_bench": 446211},
]


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--engine")
    parser.add_argument("--group", default="", help="P1 .. P5")
    parser.add_argument("--only", default="", help="one arm, for example P1.03")
    parser.add_argument("--all", action="store_true")
    parser.add_argument("--force", action="store_true", help="repeat arms that already have a result")
    parser.add_argument("--skip-preflight", action="store_true")
    parser.add_argument("--games", type=int, default=0)
    parser.add_argument("--tc", default="")
    parser.add_argument("--out-dir", default="matches")
    args = parser.parse_args()

    if not (args.group or args.only or args.all):
        raise SystemExit("choose --group P1..P5, --only P1.03 or --all")
    engine = sprt_arm.engine_path(args.engine)
    selected = [
        arm for arm in ARMS
        if args.all
        or (args.group and arm["id"].startswith(args.group + "."))
        or (args.only and arm["id"] == args.only)
    ]
    if not selected:
        raise SystemExit("no arm matches the selection")

    options = sprt_arm.handshake_options(engine)
    if options != sprt_arm.HANDSHAKE_OPTIONS:
        raise SystemExit(f"the handshake lists {options} options, expected {sprt_arm.HANDSHAKE_OPTIONS} (10 public + 60 levers)")
    if not args.skip_preflight:
        actual = sprt_arm.arm_bench(engine, "UseStableTimeManagement=true")
        if actual != DEFAULT_BENCH:
            raise SystemExit(f"default bench 12 is {actual}, the frozen total is {DEFAULT_BENCH}: wrong binary")

    failures = 0
    for arm in selected:
        result = Path(args.out_dir) / f"{arm['file']}.result.txt"
        if result.exists() and not args.force:
            print(f"[{arm['id']}] skipped: {result} exists")
            continue
        if not args.skip_preflight:
            base = sprt_arm.arm_bench(engine, arm["base"]) if arm["base"] else DEFAULT_BENCH
            with_arm = sprt_arm.arm_bench(engine, arm["arm"], arm["base"])
            if base != arm["base_bench"] or with_arm != arm["arm_bench"]:
                print(
                    f"WARNING [{arm['id']}] pre-flight mismatch: base {base} (expected {arm['base_bench']}), "
                    f"arm {with_arm} (expected {arm['arm_bench']}); not played",
                    file=sys.stderr,
                )
                failures += 1
                continue
            if arm["arm_bench"] == arm["base_bench"] and arm["id"] != "P3.01":
                print(f"WARNING [{arm['id']}] the arm prints the base total: the lever is not wired; not played", file=sys.stderr)
                failures += 1
                continue
        games = args.games or arm["games"]
        print(f"[{arm['id']}] {arm['base']} {arm['arm']}: {games} games")
        kwargs = dict(engine=engine, games=games, out_dir=args.out_dir)
        if args.tc:
            kwargs["tc"] = args.tc
        outcome = sprt_arm.run_arm(arm["file"], arm["arm"], arm["base"], **kwargs)
        failures += 0 if outcome["clean"] else 1
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
