# KnightByte - Lever Test Plan & Log

The executable test document for the sixty levers of `docs/LEVERS.md`
(`docs/decisions/ADR-0014-the-sixty-levers.md`). It lists every A/B test, in priority
order, with the exact option string, the fixed-depth pre-flight total and a
copy-paste command, and it is the log in which the results are recorded. It follows
the structure and the rules of the predecessor's test document (`audit-tests.md` of
PawnForgeCS): one variable per match, the same decision rule, a results log, a quick
reference. The tooling is KnightByte's own (`tools/sprt_arm.ps1`,
`tools/lever_campaign.ps1`) around `fastchess`.

**Status: NOT STARTED.** Nothing in the results log has been measured yet.

## Frozen snapshot

| Item | Value |
| :--- | :--- |
| Evaluator | v5 NNUE `jokausch.nnue`, sha256 `0a70169ce7d75a9a...` (the predecessor's v9 champion), embedded in the binary |
| Source | commit `a8a9359` or a later commit that changed only documents and `tools/`; **write the exact `git rev-parse --short HEAD` here when the campaign binary is built:** `__________` |
| `bench 12` default | **456,308** (pins `bench 6/12/16/18` = 15,528 / 456,308 / 2,815,799 / 6,892,874) |
| Handshake | **70 option lines**: 10 public options and 60 levers |
| Test suite | 339 tests, plus the release-mode gate `cargo test --release --test lever_effect` |
| Levers | `docs/LEVERS.md` (generated from `src/search/lever_table.rs`) |
| Arms | **60**: one primary arm per lever. P1 16, P2 9, P3 1, P4 4, P5 30 |
| Campaign binary | `knightbyte-v4-vnni.exe` from `tools\build_release.ps1` (PGO, static C runtime). Both sides of every match use this one file. **sha256:** `__________` |
| Host | Ryzen 7 260 (8 cores, 16 threads), Windows 11, high-performance power plan, AC power |

**Status legend:** `PENDING` not started | `RUNNING` | `DONE` measured, verdict recorded |
`BAKE` LLR >= +2, to be confirmed and baked after the campaign | `SKIPPED` with the reason.

**Priority legend**

| Priority | Meaning |
| :--- | :--- |
| **P0** | Prerequisites and frozen gates: run every session, not an arm |
| **P1** | 16 highest-value arms: the structural and branching-factor levers (ProbCut, the table extension, bad-noisy pruning, the quiescence correction, the LMR schedule, null move, pruning scales, quiescence narrowing) |
| **P2** | 9 ablations: does a shipped feature still pay for itself |
| **P3** | 1 timed arm: stable time management |
| **P4** | 4 ablations of the KnightByte switches |
| **P5** | 30 knobs: the remaining numeric levers and the knobs of the ported features |
| **P6** | SPSA tuning, only after a lever has won |

---

## 0. Rules and prerequisites

### 0.1 Measurement discipline

- **One variable per match.** Never bundle two arms. A knob that only acts with its
  switch (`ProbCutMargin` needs `UseProbCut`) runs with `-BaseOptions`: both sides get
  the switch, only the knob differs. The queue marks these arms *Dependency*.
- **Same binary on both sides.** The lever options go on engine `ON` only; `OFF` is the
  same file with the defaults. `Invoke-SprtArm` builds both command lines, so a second
  option string cannot be dropped silently (the predecessor's Bug 118 class).
- **`Move Overhead` 10 on both sides** (set by the helper).
- **Standard:** 400 games (200 colour-reversed pairs), `10+0.1`, **1 thread**, 256 MB,
  UHO openings in random order, `-srand 1234` (the same openings in every arm), SPRT
  0 -> 5 Elo with alpha = beta = 0.12, which gives log-likelihood bounds of +/-1.99, the
  +/-2 of the decision rule. Concurrency 6 of the 8 cores. Draw and resign adjudication
  as in the predecessor's gates (`draw movenumber=34 movecount=8 score=20`, `resign
  movecount=3 score=600`).
- **One match at a time, and nothing else on the machine.** Close browsers and builds;
  do not run benchmarks during a match. A loaded host shows up as time losses.
- **Prerequisites every session (P0):** the binary's sha256 equals the one recorded in
  the snapshot, the handshake lists 70 options, `bench 12` prints the default total,
  and each arm's pre-flight prints its recorded total. `tools/lever_campaign.ps1`
  checks all of these before it plays an arm. **A lever whose pre-flight prints the
  base total is not wired** (the predecessor's B103 class); do not play it.
- **Decision rule:** LLR >= +2 => the arm wins (`BAKE`); LLR <= -2 => keep the default,
  record the regression; in between => **inconclusive => keep the simpler default**, no
  re-run required.
- **Direction of the ablation arms (P2, P3, P4).** The arm is the *feature switched off*.
  A win (LLR >= +2) means the feature should be off by default or removed; a loss means
  it earns its place; inconclusive keeps it, because it is the shipped default.
- **Record every arm** in section 9, with the verbatim `matches\<ID>.result.txt` block and
  the log path.
- **A match is only evidence if it was clean.** `Invoke-SprtArm` counts time forfeits
  and abnormal terminations (illegal move, disconnect, crash, stall) and warns if there
  is one. A time loss in more than 1 % of the games, or any crash or illegal move, is a
  re-run signal: re-run it with `-Overhead 50` (both sides) and record the count.
- **Unattended runs have a hard cap.** `-HardMinutes` (default 360) kills `fastchess`
  and the engines. `tools/lever_campaign.ps1` resumes: an arm whose result file exists
  is skipped.
- **No code change during the campaign.** A change to the engine, a default or the
  tools' standard invalidates every verdict, because every arm is measured against the
  same frozen baseline. Wins are baked after the campaign (section 0.5).
- **Prior evidence is a hint, not a result.** The 'Prior' column quotes the predecessor's
  v9 campaign (400 games per arm, +/-22 Elo error bar, every arm inconclusive, a
  different engine and net). It says which direction was plausible, nothing more.

### 0.2 Net caveat

The network is frozen. Node totals are fixed-depth and identical on every machine and
in every build of the same commit (plain, PGO, `v2`, `v3`, `v4-vnni`), so the
pre-flight totals below hold for the campaign binary. Absolute Elo numbers belong to
this net: a different net invalidates every verdict.

### 0.3 Paths (Windows)

```powershell
cd C:\dev\knightbyte-unfinished
$env:KB_ENGINE = "C:\dev\knightbyte-unfinished\dist\knightbyte-0.1.0-windows-x64\knightbyte-v4-vnni.exe"
# optional: $env:KB_FASTCHESS (default C:\dev\fastchess-windows-x86-64\fastchess.exe)
# optional: $env:KB_BOOK      (default books\UHO_Lichess_4852_v1.epd in the repository)
```

### 0.4 Time budget

A 500-game match at `8+0.08` with 6 concurrent games took 32 min 38 s on this host (3.9 s
of wall clock per game). At `10+0.1` that is about 5 s per game: **about 33 minutes per
400-game arm and about 34 hours for all 60** (the 800-game arm adds 33 more). SPRT can
end an arm early only at a bound; the predecessor's 400-game arms never reached one. A
sensible order is the one of the queue: P1 first (about 9 hours), then P2 to P4, and P5
last.

### 0.5 After the campaign: baking

1. Collect the arms with LLR >= +2. Inconclusive arms change nothing.
2. Combine the winners in **one** confirmation match against the frozen baseline (the
   individual wins are not additive).
3. A confirmed change goes into `SearchParams::default()` and the default of the lever
   in `src/search/lever_table.rs`; the pins (`bench 6/12/16/18`), `tests/golden/search_parity.json`
   and the effect test's expectations are re-recorded; an ADR and the feature list say what
   changed and why; `python tools/levers_doc.py` regenerates `docs/LEVERS.md`.
4. Then, and only then, SPSA (section 8) on the levers of the families that won.

---

## 1. Command templates

### 1.1 Session setup

```powershell
cd C:\dev\knightbyte-unfinished
$env:KB_ENGINE = "<the campaign binary>"   # see section 0.3
. .\tools\sprt_arm.ps1                      # Invoke-SprtArm, Get-ArmBench
```

The campaign binary is built by `powershell -File tools\build_release.ps1` (it runs the
lever effect gate first, builds the three PGO variants and writes `SHA256SUMS.txt`).
Never measure a stale binary: compare the file's hash with the snapshot.

```powershell
(Get-FileHash $env:KB_ENGINE -Algorithm SHA256).Hash
```

### 1.2 One SPRT arm (the default template)

```powershell
Invoke-SprtArm -Id P1_01_UseProbCut -ArmOptions "UseProbCut=true"
```

The helper writes `matches\P1_01_UseProbCut.log`, `.pgn` and `.result.txt` and prints the
result block. Variants: more games for a small effect (`-Games 800`), a coupled knob
with its switch as the common base, and a clock-safe re-run:

```powershell
Invoke-SprtArm -Id P1_04_UseBadNoisyPruning -ArmOptions "UseBadNoisyPruning=true" -Games 800
Invoke-SprtArm -Id P5_27_ProbCutMargin -BaseOptions "UseProbCut=true" -ArmOptions "ProbCutMargin=180"
Invoke-SprtArm -Id P1_01_UseProbCut -ArmOptions "UseProbCut=true" -Overhead 50   # a re-run after time losses
```

### 1.3 The unattended queue

```powershell
.\tools\lever_campaign.ps1 -Group P1        # every arm of one priority
.\tools\lever_campaign.ps1 -Only P1.03      # one arm
.\tools\lever_campaign.ps1 -All             # the whole queue; resumes where it stopped
```

It runs the pre-flight of each arm, skips an arm whose result file exists (`-Force`
repeats it) and refuses to play an arm whose pre-flight does not match the recorded
totals of this document.

### 1.4 Smoke test

Eight short games to check the plumbing (not a result):

```powershell
Invoke-SprtArm -Id smoke -ArmOptions "UseProbCut=true" -Games 8 -Tc "3+0.03" -Concurrency 4 -OutDir matches\smoke
```

### 1.5 Pre-flight of one arm

```powershell
Get-ArmBench -ArmOptions "UseProbCut=true"                                  # depth 12 total with the arm
Get-ArmBench -ArmOptions "ProbCutMargin=180" -BaseOptions "UseProbCut=true"  # a knob against its base
& $env:KB_ENGINE param:UseProbCut=true "bench 12"                              # the same by hand
"setoption name UseProbCut value true`nbench 12`nquit" | & $env:KB_ENGINE      # or through UCI
```

`bench` runs with the levers as configured, with a 256 MB table and one thread whatever
the options say. The total of an arm must differ from the total of its base.

### 1.6 Frozen gates (P0)

```powershell
# 1. the code gates
$env:KNIGHTBYTE_FEN_VERIFIER = "C:\dev\c#-chess-engines\stockfish\stockfish-windows-x86-64-universal.exe"
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo test --release --test lever_effect      # every lever changes the search

# 2. the node pins and the per-depth identity
foreach ($d in 6,12,16,18) { "bench $d" | & $env:KB_ENGINE | Select-String "^Nodes:" }
python tools\capture_bench.py compare --engine $env:KB_ENGINE --baseline tests\golden\search_parity.json --depths 1-12,14,16,18

# 3. the handshake
("uci`nquit`n" | & $env:KB_ENGINE | Where-Object { $_ -match '^option name ' }).Count   # must be 70

# 4. the binary itself
(Get-FileHash $env:KB_ENGINE -Algorithm SHA256).Hash                                   # must match the snapshot
```

---

## 2. Priority queue at a glance

Pre-flight = `bench 12` total of the arm (the base total for a dependent arm is in the
section of the arm). Default total: **456,308**. 'Change' is the arm's total against its base.

### P1 - highest-value arms: the structural and branching-factor levers

| ID | Arm | Base | Pre-flight | Change | Games | Prior | Status |
| :--- | :--- | :--- | ---: | ---: | ---: | :--- | :--- |
| P1.01 | `UseProbCut=true` |  | 442,190 | -3.1 % | 400 | ProbCut with its secondary layer: -4.3 | PENDING |
| P1.02 | `UseTtExtension=true` |  | 502,408 | +10.1 % | 400 | +9.6 | PENDING |
| P1.03 | `UseQsCorrectionHistory=true` |  | 536,219 | +17.5 % | 400 | +8.7 | PENDING |
| P1.04 | `UseBadNoisyPruning=true` |  | 445,981 | -2.3 % | 800 | +4.8 (800 games) | PENDING |
| P1.05 | `LmrDivisor=225` |  | 573,915 | +25.8 % | 400 |  | PENDING |
| P1.06 | `LmrHistoryDivisor=2500` |  | 442,829 | -3.0 % | 400 |  | PENDING |
| P1.07 | `LmrPvWeight=600` |  | 413,497 | -9.4 % | 400 |  | PENDING |
| P1.08 | `LmrKillerWeight=600` |  | 518,382 | +13.6 % | 400 |  | PENDING |
| P1.09 | `LmrMinMoves=2` |  | 370,559 | -18.8 % | 400 |  | PENDING |
| P1.10 | `UseTerm1NotImproving=true` |  | 330,643 | -27.5 % | 400 |  | PENDING |
| P1.11 | `NmpBaseReduction=4` |  | 380,463 | -16.6 % | 400 |  | PENDING |
| P1.12 | `NmpEvalDivisor=140` |  | 388,793 | -14.8 % | 400 |  | PENDING |
| P1.13 | `RfpNotImprovingScale=80` |  | 440,620 | -3.4 % | 400 |  | PENDING |
| P1.14 | `FutilityScale=85` |  | 532,647 | +16.7 % | 400 |  | PENDING |
| P1.15 | `LmpScale=80` |  | 442,123 | -3.1 % | 400 |  | PENDING |
| P1.16 | `UseQsNarrowing=true` |  | 428,894 | -6.0 % | 400 | +5.2 | PENDING |

### P2 - ablations: does a shipped feature still pay for itself

| ID | Arm | Base | Pre-flight | Change | Games | Prior | Status |
| :--- | :--- | :--- | ---: | ---: | ---: | :--- | :--- |
| P2.01 | `UseTtScoreEval=false` |  | 388,414 | -14.9 % | 400 | off: -8.7 | PENDING |
| P2.02 | `UseMicroRiders=false` |  | 437,184 | -4.2 % | 400 | off: -9.6 | PENDING |
| P2.03 | `UseImprovingFlag=false` |  | 603,732 | +32.3 % | 400 | off: -10.4 | PENDING |
| P2.04 | `UseCutNode=false` |  | 436,287 | -4.4 % | 400 | off: +2.6 | PENDING |
| P2.05 | `UseNegativeExtensions=false` |  | 439,126 | -3.8 % | 400 | off: 0.0 | PENDING |
| P2.06 | `UseInternalIterativeReduction=false` |  | 480,101 | +5.2 % | 400 |  | PENDING |
| P2.07 | `UseTrioHindsight=false` |  | 443,994 | -2.7 % | 400 |  | PENDING |
| P2.08 | `UseHistoryPruning=false` |  | 537,894 | +17.9 % | 400 |  | PENDING |
| P2.09 | `UseReSearchReduction=false` |  | 553,573 | +21.3 % | 400 |  | PENDING |

### P3 - the timed lever

| ID | Arm | Base | Pre-flight | Change | Games | Prior | Status |
| :--- | :--- | :--- | ---: | ---: | ---: | :--- | :--- |
| P3.01 | `UseStableTimeManagement=false` |  | 456,308 | flat (by design) | 400 | off: -24.4 | PENDING |

### P4 - ablations of the KnightByte switches

| ID | Arm | Base | Pre-flight | Change | Games | Prior | Status |
| :--- | :--- | :--- | ---: | ---: | ---: | :--- | :--- |
| P4.01 | `UseCorrectionHistory=false` |  | 484,853 | +6.3 % | 400 |  | PENDING |
| P4.02 | `UseCheckAwareQuiescence=false` |  | 422,703 | -7.4 % | 400 |  | PENDING |
| P4.03 | `UseSeeApproxPrefilter=false` |  | 340,948 | -25.3 % | 400 |  | PENDING |
| P4.04 | `UseQsTtProbe=false` |  | 449,325 | -1.5 % | 400 | off: -0.9 | PENDING |

### P5 - the remaining knobs

| ID | Arm | Base | Pre-flight | Change | Games | Prior | Status |
| :--- | :--- | :--- | ---: | ---: | ---: | :--- | :--- |
| P5.01 | `AspirationBase=33` |  | 647,408 | +41.9 % | 400 | =33: +6.1 | PENDING |
| P5.02 | `AspirationStartDepth=7` |  | 368,657 | -19.2 % | 400 |  | PENDING |
| P5.03 | `NmpDepthDivisor=3` |  | 450,679 | -1.2 % | 400 | =2: -27.0 | PENDING |
| P5.04 | `NmpEvalBonusMax=0` |  | 444,762 | -2.5 % | 400 | eval scale on: +12.2 | PENDING |
| P5.05 | `NmpMinDepth=4` |  | 464,233 | +1.7 % | 400 |  | PENDING |
| P5.06 | `RfpFormulaMaxDepth=10` |  | 455,530 | -0.2 % | 400 |  | PENDING |
| P5.07 | `RfpImprovingScale=55` |  | 515,700 | +13.0 % | 400 |  | PENDING |
| P5.08 | `RfpDamping=2` |  | 438,965 | -3.8 % | 400 |  | PENDING |
| P5.09 | `RazorMaxDepth=4` |  | 457,019 | +0.2 % | 400 |  | PENDING |
| P5.10 | `RazorMarginScale=80` |  | 563,540 | +23.5 % | 400 |  | PENDING |
| P5.11 | `FutilityMaxDepth=9` |  | 442,770 | -3.0 % | 400 |  | PENDING |
| P5.12 | `LmpMaxDepth=6` |  | 438,259 | -4.0 % | 400 |  | PENDING |
| P5.13 | `HistPruneMaxDepth=1` |  | 422,845 | -7.3 % | 400 |  | PENDING |
| P5.14 | `HistPruneMargin=1500` |  | 425,556 | -6.7 % | 400 |  | PENDING |
| P5.15 | `SeePruningMaxDepth=10` |  | 451,803 | -1.0 % | 400 | SEE thresholds with depth 8 off: -8.7 | PENDING |
| P5.16 | `SeeQuietMargin=-30` |  | 406,602 | -10.9 % | 400 |  | PENDING |
| P5.17 | `SeeNoisyMargin=-18` |  | 467,679 | +2.5 % | 400 |  | PENDING |
| P5.18 | `SeeHistoryDivisor=192` |  | 485,273 | +6.3 % | 400 |  | PENDING |
| P5.19 | `NegExtHistoryThreshold=2000` |  | 475,746 | +4.3 % | 400 |  | PENDING |
| P5.20 | `IirMinDepth=6` |  | 392,859 | -13.9 % | 400 |  | PENDING |
| P5.21 | `HindsightMargin=30` |  | 510,289 | +11.8 % | 400 |  | PENDING |
| P5.22 | `LmrMinDepth=2` |  | 460,888 | +1.0 % | 400 |  | PENDING |
| P5.23 | `QsDeltaMargin=900` |  | 453,686 | -0.6 % | 400 |  | PENDING |
| P5.24 | `QsRecaptureOnlyPly=4` | `UseQsNarrowing=true` | 437,102 | +1.9 % | 400 |  | PENDING |
| P5.25 | `HistoryBonusScale=130` |  | 434,036 | -4.9 % | 400 |  | PENDING |
| P5.26 | `HistoryMalusScale=130` |  | 425,788 | -6.7 % | 400 |  | PENDING |
| P5.27 | `ProbCutMargin=180` | `UseProbCut=true` | 404,945 | -8.4 % | 400 |  | PENDING |
| P5.28 | `ProbCutMinDepth=7` | `UseProbCut=true` | 459,622 | +3.9 % | 400 |  | PENDING |
| P5.29 | `TtExtensionMaxDepth=8` | `UseTtExtension=true` | 538,355 | +7.2 % | 400 |  | PENDING |
| P5.30 | `BnpDepth=4` | `UseBadNoisyPruning=true` | 446,211 | +0.1 % | 400 |  | PENDING |

### P6 - SPSA tuning (only after a lever has won)

No arm here; see section 8.

---

## 3. P1 - highest-value arms: the structural and branching-factor levers

These are the arms most likely to matter. Run them first and in this order. The first four are the ported features and the quiescence correction (the predecessor's best default-off estimates, and ProbCut for the branching factor); then the LMR schedule, null move and the pruning scales, which are the plan's first EBF experiments.

### P1.01 - `UseProbCut=true`

- **Gate:** SPRT. **Pre-flight:** 442,190 (default 456,308); change -3.1 %.
- **Hypothesis:** The one structural change aimed at the branching factor: a capture that survives quiescence and a reduced search above beta + margin ends the node early.
- **Prior evidence (predecessor, v9):** ProbCut with its secondary layer: -4.3 Elo.
- **Lever:** `UseProbCut`, default false, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P1_01_UseProbCut -ArmOptions "UseProbCut=true"
```

- **Status:** PENDING.

### P1.02 - `UseTtExtension=true`

- **Gate:** SPRT. **Pre-flight:** 502,408 (default 456,308); change +10.1 %.
- **Hypothesis:** A deep table entry that lies outside the window earns one extra ply; the best default-off estimate of the predecessor campaign.
- **Prior evidence (predecessor, v9):** +9.6 Elo.
- **Lever:** `UseTtExtension`, default false, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P1_02_UseTtExtension -ArmOptions "UseTtExtension=true"
```

- **Status:** PENDING.

### P1.03 - `UseQsCorrectionHistory=true`

- **Gate:** SPRT. **Pre-flight:** 536,219 (default 456,308); change +17.5 %.
- **Hypothesis:** The quiescence stand-pat gets the same learned correction as the main search, so the score does not jump at the boundary.
- **Prior evidence (predecessor, v9):** +8.7 Elo.
- **Lever:** `UseQsCorrectionHistory`, default false, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P1_03_UseQsCorrectionHistory -ArmOptions "UseQsCorrectionHistory=true"
```

- **Status:** PENDING.

### P1.04 - `UseBadNoisyPruning=true`

- **Gate:** SPRT, 800 games. **Pre-flight:** 445,981 (default 456,308); change -2.3 %.
- **Hypothesis:** The first losing capture of a hopeless node ends the move loop. Small effect: run 800 games.
- **Prior evidence (predecessor, v9):** +4.8 (800 games) Elo.
- **Lever:** `UseBadNoisyPruning`, default false, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P1_04_UseBadNoisyPruning -ArmOptions "UseBadNoisyPruning=true" -Games 800
```

- **Status:** PENDING.

### P1.05 - `LmrDivisor=225`

- **Gate:** SPRT. **Pre-flight:** 573,915 (default 456,308); change +25.8 %.
- **Hypothesis:** A larger divisor (2.25 instead of 1.75) gives a milder reduction schedule; the plan's first EBF experiment. Test the other direction (150) after this one.
- **Lever:** `LmrDivisor`, default 175, range 100 to 300.
- **Command:**

```powershell
Invoke-SprtArm -Id P1_05_LmrDivisor -ArmOptions "LmrDivisor=225"
```

- **Status:** PENDING.

### P1.06 - `LmrHistoryDivisor=2500`

- **Gate:** SPRT. **Pre-flight:** 442,829 (default 456,308); change -3.0 %.
- **Hypothesis:** A smaller divisor lets the history of a move change its reduction more: good moves are reduced less, bad ones more.
- **Lever:** `LmrHistoryDivisor`, default 3419, range 1000 to 8000.
- **Command:**

```powershell
Invoke-SprtArm -Id P1_06_LmrHistoryDivisor -ArmOptions "LmrHistoryDivisor=2500"
```

- **Status:** PENDING.

### P1.07 - `LmrPvWeight=600`

- **Gate:** SPRT. **Pre-flight:** 413,497 (default 456,308); change -9.4 %.
- **Hypothesis:** Principal-variation nodes get less credit against the reduction (600 instead of 1024 of a ply): late moves are reduced more everywhere.
- **Lever:** `LmrPvWeight`, default 1024, range 0 to 2048.
- **Command:**

```powershell
Invoke-SprtArm -Id P1_07_LmrPvWeight -ArmOptions "LmrPvWeight=600"
```

- **Status:** PENDING.

### P1.08 - `LmrKillerWeight=600`

- **Gate:** SPRT. **Pre-flight:** 518,382 (default 456,308); change +13.6 %.
- **Hypothesis:** Killers and countermoves get less credit against the reduction (600 instead of 1024): they are reduced more.
- **Lever:** `LmrKillerWeight`, default 1024, range 0 to 2048.
- **Command:**

```powershell
Invoke-SprtArm -Id P1_08_LmrKillerWeight -ArmOptions "LmrKillerWeight=600"
```

- **Status:** PENDING.

### P1.09 - `LmrMinMoves=2`

- **Gate:** SPRT. **Pre-flight:** 370,559 (default 456,308); change -18.8 %.
- **Hypothesis:** Reductions start at the third move instead of the fourth: a smaller tree if the move ordering is good enough.
- **Lever:** `LmrMinMoves`, default 3, range 1 to 8.
- **Command:**

```powershell
Invoke-SprtArm -Id P1_09_LmrMinMoves -ArmOptions "LmrMinMoves=2"
```

- **Status:** PENDING.

### P1.10 - `UseTerm1NotImproving=true`

- **Gate:** SPRT. **Pre-flight:** 330,643 (default 456,308); change -27.5 %.
- **Hypothesis:** One more ply of reduction when the position is not improving; standard in many engines, off in the reference.
- **Lever:** `UseTerm1NotImproving`, default false, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P1_10_UseTerm1NotImproving -ArmOptions "UseTerm1NotImproving=true"
```

- **Status:** PENDING.

### P1.11 - `NmpBaseReduction=4`

- **Gate:** SPRT. **Pre-flight:** 380,463 (default 456,308); change -16.6 %.
- **Hypothesis:** A deeper null-move search reduction: fewer nodes below a null move, at some risk in zugzwang positions.
- **Lever:** `NmpBaseReduction`, default 3, range 1 to 6.
- **Command:**

```powershell
Invoke-SprtArm -Id P1_11_NmpBaseReduction -ArmOptions "NmpBaseReduction=4"
```

- **Status:** PENDING.

### P1.12 - `NmpEvalDivisor=140`

- **Gate:** SPRT. **Pre-flight:** 388,793 (default 456,308); change -14.8 %.
- **Hypothesis:** A smaller divisor makes the evaluation surplus add to the null-move reduction faster.
- **Lever:** `NmpEvalDivisor`, default 191, range 50 to 400.
- **Command:**

```powershell
Invoke-SprtArm -Id P1_12_NmpEvalDivisor -ArmOptions "NmpEvalDivisor=140"
```

- **Status:** PENDING.

### P1.13 - `RfpNotImprovingScale=80`

- **Gate:** SPRT. **Pre-flight:** 440,620 (default 456,308); change -3.4 %.
- **Hypothesis:** A smaller margin prunes more at nodes that are not improving.
- **Lever:** `RfpNotImprovingScale`, default 95, range 10 to 300.
- **Command:**

```powershell
Invoke-SprtArm -Id P1_13_RfpNotImprovingScale -ArmOptions "RfpNotImprovingScale=80"
```

- **Status:** PENDING.

### P1.14 - `FutilityScale=85`

- **Gate:** SPRT. **Pre-flight:** 532,647 (default 456,308); change +16.7 %.
- **Hypothesis:** All futility margins 15 percent smaller: more quiet moves pruned near the leaves.
- **Lever:** `FutilityScale`, default 100, range 25 to 300.
- **Command:**

```powershell
Invoke-SprtArm -Id P1_14_FutilityScale -ArmOptions "FutilityScale=85"
```

- **Status:** PENDING.

### P1.15 - `LmpScale=80`

- **Gate:** SPRT. **Pre-flight:** 442,123 (default 456,308); change -3.1 %.
- **Hypothesis:** Late-move-pruning limits 20 percent lower: fewer quiet moves tried at shallow depth.
- **Lever:** `LmpScale`, default 100, range 25 to 300.
- **Command:**

```powershell
Invoke-SprtArm -Id P1_15_LmpScale -ArmOptions "LmpScale=80"
```

- **Status:** PENDING.

### P1.16 - `UseQsNarrowing=true`

- **Gate:** SPRT. **Pre-flight:** 428,894 (default 456,308); change -6.0 %.
- **Hypothesis:** Quiescence searches only recaptures from ply 8 and no promotions from ply 2: a smaller quiescence tree.
- **Prior evidence (predecessor, v9):** +5.2 Elo.
- **Lever:** `UseQsNarrowing`, default false, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P1_16_UseQsNarrowing -ArmOptions "UseQsNarrowing=true"
```

- **Status:** PENDING.

---

## 4. P2 - ablations: does a shipped feature still pay for itself

Each arm switches a shipped feature off. The predecessor measured most of them as worth keeping; a win here would be a surprise and would remove code.

### P2.01 - `UseTtScoreEval=false`

- **Gate:** SPRT. **Pre-flight:** 388,414 (default 456,308); change -14.9 %.
- **Hypothesis:** Ablation: a consistent table score is no longer reused as the evaluation. A win here means the feature should go.
- **Prior evidence (predecessor, v9):** off: -8.7 Elo.
- **Lever:** `UseTtScoreEval`, default true, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P2_01_UseTtScoreEval -ArmOptions "UseTtScoreEval=false"
```

- **Status:** PENDING.

### P2.02 - `UseMicroRiders=false`

- **Gate:** SPRT. **Pre-flight:** 437,184 (default 456,308); change -4.2 %.
- **Hypothesis:** Ablation: quiescence returns beta on a fail-high instead of averaging.
- **Prior evidence (predecessor, v9):** off: -9.6 Elo.
- **Lever:** `UseMicroRiders`, default true, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P2_02_UseMicroRiders -ArmOptions "UseMicroRiders=false"
```

- **Status:** PENDING.

### P2.03 - `UseImprovingFlag=false`

- **Gate:** SPRT. **Pre-flight:** 603,732 (default 456,308); change +32.3 %.
- **Hypothesis:** Ablation: the improving flag is no longer computed (every node counts as not improving).
- **Prior evidence (predecessor, v9):** off: -10.4 Elo.
- **Lever:** `UseImprovingFlag`, default true, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P2_03_UseImprovingFlag -ArmOptions "UseImprovingFlag=false"
```

- **Status:** PENDING.

### P2.04 - `UseCutNode=false`

- **Gate:** SPRT. **Pre-flight:** 436,287 (default 456,308); change -4.4 %.
- **Hypothesis:** Ablation: no cut-node prediction in the reductions and the null-move gate.
- **Prior evidence (predecessor, v9):** off: +2.6 Elo.
- **Lever:** `UseCutNode`, default true, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P2_04_UseCutNode -ArmOptions "UseCutNode=false"
```

- **Status:** PENDING.

### P2.05 - `UseNegativeExtensions=false`

- **Gate:** SPRT. **Pre-flight:** 439,126 (default 456,308); change -3.8 %.
- **Hypothesis:** Ablation: suspicious moves are no longer searched a ply shallower.
- **Prior evidence (predecessor, v9):** off: 0.0 Elo.
- **Lever:** `UseNegativeExtensions`, default true, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P2_05_UseNegativeExtensions -ArmOptions "UseNegativeExtensions=false"
```

- **Status:** PENDING.

### P2.06 - `UseInternalIterativeReduction=false`

- **Gate:** SPRT. **Pre-flight:** 480,101 (default 456,308); change +5.2 %.
- **Hypothesis:** Ablation: a node without a table move is no longer reduced.
- **Lever:** `UseInternalIterativeReduction`, default true, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P2_06_UseInternalIterativeReduction -ArmOptions "UseInternalIterativeReduction=false"
```

- **Status:** PENDING.

### P2.07 - `UseTrioHindsight=false`

- **Gate:** SPRT. **Pre-flight:** 443,994 (default 456,308); change -2.7 %.
- **Hypothesis:** Ablation: the hindsight depth pair (reduced parents earn or lose a ply) is switched off.
- **Lever:** `UseTrioHindsight`, default true, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P2_07_UseTrioHindsight -ArmOptions "UseTrioHindsight=false"
```

- **Status:** PENDING.

### P2.08 - `UseHistoryPruning=false`

- **Gate:** SPRT. **Pre-flight:** 537,894 (default 456,308); change +17.9 %.
- **Hypothesis:** Ablation: quiet moves with very poor history are no longer pruned at low depth.
- **Lever:** `UseHistoryPruning`, default true, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P2_08_UseHistoryPruning -ArmOptions "UseHistoryPruning=false"
```

- **Status:** PENDING.

### P2.09 - `UseReSearchReduction=false`

- **Gate:** SPRT. **Pre-flight:** 553,573 (default 456,308); change +21.3 %.
- **Hypothesis:** Ablation: the re-search after a reduced fail-high is no longer shaved.
- **Lever:** `UseReSearchReduction`, default true, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P2_09_UseReSearchReduction -ArmOptions "UseReSearchReduction=false"
```

- **Status:** PENDING.

---

## 5. P3 - the timed lever

The only lever that a fixed-depth search cannot see. All matches here are timed, so it is run like any other arm; its pre-flight is flat by design. A clock-safe run (zero time losses) matters most for this one.

### P3.01 - `UseStableTimeManagement=false`

- **Gate:** TIMED SPRT. **Pre-flight:** 456,308 (default 456,308); change flat (by design).
- **Hypothesis:** Ablation, timed: the three-signal soft-limit scaler is replaced by the legacy multipliers. Only a clock sees this lever, so the pre-flight is flat by design.
- **Prior evidence (predecessor, v9):** off: -24.4 Elo.
- **Lever:** `UseStableTimeManagement`, default true, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P3_01_UseStableTimeManagement -ArmOptions "UseStableTimeManagement=false"
```

- **Status:** PENDING.

---

## 6. P4 - ablations of the KnightByte switches

The three KnightByte switches (features the reference always runs) and the quiescence table probe, ablated one at a time. If a switch wins, the feature is worth nothing and goes, together with its lever.

### P4.01 - `UseCorrectionHistory=false`

- **Gate:** SPRT. **Pre-flight:** 484,853 (default 456,308); change +6.3 %.
- **Hypothesis:** Ablation: no learned static-evaluation correction at all.
- **Lever:** `UseCorrectionHistory`, default true, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P4_01_UseCorrectionHistory -ArmOptions "UseCorrectionHistory=false"
```

- **Status:** PENDING.

### P4.02 - `UseCheckAwareQuiescence=false`

- **Gate:** SPRT. **Pre-flight:** 422,703 (default 456,308); change -7.4 %.
- **Hypothesis:** Ablation: quiescence no longer searches every evasion when the king is attacked.
- **Lever:** `UseCheckAwareQuiescence`, default true, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P4_02_UseCheckAwareQuiescence -ArmOptions "UseCheckAwareQuiescence=false"
```

- **Status:** PENDING.

### P4.03 - `UseSeeApproxPrefilter=false`

- **Gate:** SPRT. **Pre-flight:** 340,948 (default 456,308); change -25.3 %.
- **Hypothesis:** Ablation: the cheap SEE prefilter of quiescence captures is removed (slower but exact).
- **Lever:** `UseSeeApproxPrefilter`, default true, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P4_03_UseSeeApproxPrefilter -ArmOptions "UseSeeApproxPrefilter=false"
```

- **Status:** PENDING.

### P4.04 - `UseQsTtProbe=false`

- **Gate:** SPRT. **Pre-flight:** 449,325 (default 456,308); change -1.5 %.
- **Hypothesis:** Ablation: quiescence no longer probes the transposition table.
- **Prior evidence (predecessor, v9):** off: -0.9 Elo.
- **Lever:** `UseQsTtProbe`, default true, range true / false.
- **Command:**

```powershell
Invoke-SprtArm -Id P4_04_UseQsTtProbe -ArmOptions "UseQsTtProbe=false"
```

- **Status:** PENDING.

---

## 7. P5 - the remaining knobs

Single numeric knobs, each moved to one value on the more aggressive side (smaller margins and divisors, deeper gates), except where the note says otherwise. A knob whose first value loses may still win in the other direction: the second direction belongs to SPSA (section 8). Near-null effects (a few tenths of a percent of the nodes) cannot be resolved by 400 games; the pre-flight column shows which arms those are.

### P5.01 - `AspirationBase=33`

- **Gate:** SPRT. **Pre-flight:** 647,408 (default 456,308); change +41.9 %.
- **Hypothesis:** A tighter first aspiration window.
- **Prior evidence (predecessor, v9):** =33: +6.1 Elo.
- **Lever:** `AspirationBase`, default 50, range 10 to 100.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_01_AspirationBase -ArmOptions "AspirationBase=33"
```

- **Status:** PENDING.

### P5.02 - `AspirationStartDepth=7`

- **Gate:** SPRT. **Pre-flight:** 368,657 (default 456,308); change -19.2 %.
- **Hypothesis:** Aspiration windows start later, so volatile shallow iterations search the full window.
- **Lever:** `AspirationStartDepth`, default 5, range 1 to 10.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_02_AspirationStartDepth -ArmOptions "AspirationStartDepth=7"
```

- **Status:** PENDING.

### P5.03 - `NmpDepthDivisor=3`

- **Gate:** SPRT. **Pre-flight:** 450,679 (default 456,308); change -1.2 %.
- **Hypothesis:** The null-move reduction grows faster with depth. The value 2 measured -27.0 in the predecessor campaign; 3 is the milder step.
- **Prior evidence (predecessor, v9):** =2: -27.0 Elo.
- **Lever:** `NmpDepthDivisor`, default 4, range 1 to 8.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_03_NmpDepthDivisor -ArmOptions "NmpDepthDivisor=3"
```

- **Status:** PENDING.

### P5.04 - `NmpEvalBonusMax=0`

- **Gate:** SPRT. **Pre-flight:** 444,762 (default 456,308); change -2.5 %.
- **Hypothesis:** No evaluation bonus in the null-move reduction at all: tests what the eval-scaled part is worth (the predecessor baked it in after a +12.2 estimate).
- **Prior evidence (predecessor, v9):** eval scale on: +12.2 Elo.
- **Lever:** `NmpEvalBonusMax`, default 4, range 0 to 8.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_04_NmpEvalBonusMax -ArmOptions "NmpEvalBonusMax=0"
```

- **Status:** PENDING.

### P5.05 - `NmpMinDepth=4`

- **Gate:** SPRT. **Pre-flight:** 464,233 (default 456,308); change +1.7 %.
- **Hypothesis:** The null move is not tried at depth 3.
- **Lever:** `NmpMinDepth`, default 3, range 2 to 8.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_05_NmpMinDepth -ArmOptions "NmpMinDepth=4"
```

- **Status:** PENDING.

### P5.06 - `RfpFormulaMaxDepth=10`

- **Gate:** SPRT. **Pre-flight:** 455,530 (default 456,308); change -0.2 %.
- **Hypothesis:** Reverse futility applies two plies deeper.
- **Lever:** `RfpFormulaMaxDepth`, default 8, range 4 to 12.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_06_RfpFormulaMaxDepth -ArmOptions "RfpFormulaMaxDepth=10"
```

- **Status:** PENDING.

### P5.07 - `RfpImprovingScale=55`

- **Gate:** SPRT. **Pre-flight:** 515,700 (default 456,308); change +13.0 %.
- **Hypothesis:** A smaller margin prunes more at improving nodes.
- **Lever:** `RfpImprovingScale`, default 65, range 10 to 200.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_07_RfpImprovingScale -ArmOptions "RfpImprovingScale=55"
```

- **Status:** PENDING.

### P5.08 - `RfpDamping=2`

- **Gate:** SPRT. **Pre-flight:** 438,965 (default 456,308); change -3.8 %.
- **Hypothesis:** A reverse-futility cutoff returns half of the overshoot above beta instead of a third.
- **Lever:** `RfpDamping`, default 3, range 1 to 8.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_08_RfpDamping -ArmOptions "RfpDamping=2"
```

- **Status:** PENDING.

### P5.09 - `RazorMaxDepth=4`

- **Gate:** SPRT. **Pre-flight:** 457,019 (default 456,308); change +0.2 %.
- **Hypothesis:** Razoring applies one ply deeper.
- **Lever:** `RazorMaxDepth`, default 3, range 2 to 8.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_09_RazorMaxDepth -ArmOptions "RazorMaxDepth=4"
```

- **Status:** PENDING.

### P5.10 - `RazorMarginScale=80`

- **Gate:** SPRT. **Pre-flight:** 563,540 (default 456,308); change +23.5 %.
- **Hypothesis:** Razoring margins 20 percent smaller: more nodes dropped into quiescence.
- **Lever:** `RazorMarginScale`, default 100, range 25 to 300.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_10_RazorMarginScale -ArmOptions "RazorMarginScale=80"
```

- **Status:** PENDING.

### P5.11 - `FutilityMaxDepth=9`

- **Gate:** SPRT. **Pre-flight:** 442,770 (default 456,308); change -3.0 %.
- **Hypothesis:** Futility pruning applies two plies deeper.
- **Lever:** `FutilityMaxDepth`, default 7, range 4 to 10.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_11_FutilityMaxDepth -ArmOptions "FutilityMaxDepth=9"
```

- **Status:** PENDING.

### P5.12 - `LmpMaxDepth=6`

- **Gate:** SPRT. **Pre-flight:** 438,259 (default 456,308); change -4.0 %.
- **Hypothesis:** Late move pruning applies two plies deeper.
- **Lever:** `LmpMaxDepth`, default 4, range 2 to 8.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_12_LmpMaxDepth -ArmOptions "LmpMaxDepth=6"
```

- **Status:** PENDING.

### P5.13 - `HistPruneMaxDepth=1`

- **Gate:** SPRT. **Pre-flight:** 422,845 (default 456,308); change -7.3 %.
- **Hypothesis:** History pruning only at depth 1. (Above depth 4 it can never fire at the default margin: history scores are clamped to 8,033 and the threshold is 2,000 times the depth.)
- **Lever:** `HistPruneMaxDepth`, default 3, range 1 to 8.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_13_HistPruneMaxDepth -ArmOptions "HistPruneMaxDepth=1"
```

- **Status:** PENDING.

### P5.14 - `HistPruneMargin=1500`

- **Gate:** SPRT. **Pre-flight:** 425,556 (default 456,308); change -6.7 %.
- **Hypothesis:** A smaller margin: a quiet move needs less poor history to be pruned.
- **Lever:** `HistPruneMargin`, default 2000, range 800 to 4000.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_14_HistPruneMargin -ArmOptions "HistPruneMargin=1500"
```

- **Status:** PENDING.

### P5.15 - `SeePruningMaxDepth=10`

- **Gate:** SPRT. **Pre-flight:** 451,803 (default 456,308); change -1.0 %.
- **Hypothesis:** SEE pruning applies two plies deeper.
- **Prior evidence (predecessor, v9):** SEE thresholds with depth 8 off: -8.7 Elo.
- **Lever:** `SeePruningMaxDepth`, default 8, range 2 to 10.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_15_SeePruningMaxDepth -ArmOptions "SeePruningMaxDepth=10"
```

- **Status:** PENDING.

### P5.16 - `SeeQuietMargin=-30`

- **Gate:** SPRT. **Pre-flight:** 406,602 (default 456,308); change -10.9 %.
- **Hypothesis:** Less tolerance for losing quiet moves: more SEE pruning.
- **Lever:** `SeeQuietMargin`, default -43, range -120 to 0.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_16_SeeQuietMargin -ArmOptions "SeeQuietMargin=-30"
```

- **Status:** PENDING.

### P5.17 - `SeeNoisyMargin=-18`

- **Gate:** SPRT. **Pre-flight:** 467,679 (default 456,308); change +2.5 %.
- **Hypothesis:** Less tolerance for losing captures: more SEE pruning.
- **Lever:** `SeeNoisyMargin`, default -26, range -100 to 0.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_17_SeeNoisyMargin -ArmOptions "SeeNoisyMargin=-18"
```

- **Status:** PENDING.

### P5.18 - `SeeHistoryDivisor=192`

- **Gate:** SPRT. **Pre-flight:** 485,273 (default 456,308); change +6.3 %.
- **Hypothesis:** A weaker history refund in the SEE threshold: more SEE pruning.
- **Lever:** `SeeHistoryDivisor`, default 128, range 32 to 512.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_18_SeeHistoryDivisor -ArmOptions "SeeHistoryDivisor=192"
```

- **Status:** PENDING.

### P5.19 - `NegExtHistoryThreshold=2000`

- **Gate:** SPRT. **Pre-flight:** 475,746 (default 456,308); change +4.3 %.
- **Hypothesis:** More quiet moves count as suspicious and are searched a ply shallower.
- **Lever:** `NegExtHistoryThreshold`, default 3000, range 500 to 8000.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_19_NegExtHistoryThreshold -ArmOptions "NegExtHistoryThreshold=2000"
```

- **Status:** PENDING.

### P5.20 - `IirMinDepth=6`

- **Gate:** SPRT. **Pre-flight:** 392,859 (default 456,308); change -13.9 %.
- **Hypothesis:** Internal iterative reduction only at depth 6 and more.
- **Lever:** `IirMinDepth`, default 4, range 2 to 10.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_20_IirMinDepth -ArmOptions "IirMinDepth=6"
```

- **Status:** PENDING.

### P5.21 - `HindsightMargin=30`

- **Gate:** SPRT. **Pre-flight:** 510,289 (default 456,308); change +11.8 %.
- **Hypothesis:** A parent loses a ply on a smaller evaluation sum: more depth reduction in hindsight.
- **Lever:** `HindsightMargin`, default 62, range 0 to 200.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_21_HindsightMargin -ArmOptions "HindsightMargin=30"
```

- **Status:** PENDING.

### P5.22 - `LmrMinDepth=2`

- **Gate:** SPRT. **Pre-flight:** 460,888 (default 456,308); change +1.0 %.
- **Hypothesis:** Late moves are reduced from depth 2 instead of 3.
- **Lever:** `LmrMinDepth`, default 3, range 2 to 8.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_22_LmrMinDepth -ArmOptions "LmrMinDepth=2"
```

- **Status:** PENDING.

### P5.23 - `QsDeltaMargin=900`

- **Gate:** SPRT. **Pre-flight:** 453,686 (default 456,308); change -0.6 %.
- **Hypothesis:** A tighter delta-pruning margin in quiescence: more captures pruned.
- **Lever:** `QsDeltaMargin`, default 1100, range 700 to 1500.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_23_QsDeltaMargin -ArmOptions "QsDeltaMargin=900"
```

- **Status:** PENDING.

### P5.24 - `QsRecaptureOnlyPly=4` (with `UseQsNarrowing=true`)

- **Gate:** SPRT. **Pre-flight:** 437,102 (the base `UseQsNarrowing=true` alone: 428,894); change +1.9 %.
- **Hypothesis:** With narrowing on, only recaptures from ply 4 instead of 8.
- **Lever:** `QsRecaptureOnlyPly`, default 8, range 0 to 16.
- **Dependency:** acts only with `UseQsNarrowing=true`; both sides run with it (`-BaseOptions`).
- **Command:**

```powershell
Invoke-SprtArm -Id P5_24_QsRecaptureOnlyPly -BaseOptions "UseQsNarrowing=true" -ArmOptions "QsRecaptureOnlyPly=4"
```

- **Status:** PENDING.

### P5.25 - `HistoryBonusScale=130`

- **Gate:** SPRT. **Pre-flight:** 434,036 (default 456,308); change -4.9 %.
- **Hypothesis:** History bonuses 30 percent larger: move ordering adapts faster.
- **Lever:** `HistoryBonusScale`, default 100, range 25 to 300.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_25_HistoryBonusScale -ArmOptions "HistoryBonusScale=130"
```

- **Status:** PENDING.

### P5.26 - `HistoryMalusScale=130`

- **Gate:** SPRT. **Pre-flight:** 425,788 (default 456,308); change -6.7 %.
- **Hypothesis:** History maluses 30 percent larger.
- **Lever:** `HistoryMalusScale`, default 100, range 25 to 300.
- **Command:**

```powershell
Invoke-SprtArm -Id P5_26_HistoryMalusScale -ArmOptions "HistoryMalusScale=130"
```

- **Status:** PENDING.

### P5.27 - `ProbCutMargin=180` (with `UseProbCut=true`)

- **Gate:** SPRT. **Pre-flight:** 404,945 (the base `UseProbCut=true` alone: 442,190); change -8.4 %.
- **Hypothesis:** With ProbCut on, a smaller margin: more cutoffs, more risk.
- **Lever:** `ProbCutMargin`, default 235, range 100 to 400.
- **Dependency:** acts only with `UseProbCut=true`; both sides run with it (`-BaseOptions`).
- **Command:**

```powershell
Invoke-SprtArm -Id P5_27_ProbCutMargin -BaseOptions "UseProbCut=true" -ArmOptions "ProbCutMargin=180"
```

- **Status:** PENDING.

### P5.28 - `ProbCutMinDepth=7` (with `UseProbCut=true`)

- **Gate:** SPRT. **Pre-flight:** 459,622 (the base `UseProbCut=true` alone: 442,190); change +3.9 %.
- **Hypothesis:** With ProbCut on, only at depth 7 and more.
- **Lever:** `ProbCutMinDepth`, default 5, range 4 to 10.
- **Dependency:** acts only with `UseProbCut=true`; both sides run with it (`-BaseOptions`).
- **Command:**

```powershell
Invoke-SprtArm -Id P5_28_ProbCutMinDepth -BaseOptions "UseProbCut=true" -ArmOptions "ProbCutMinDepth=7"
```

- **Status:** PENDING.

### P5.29 - `TtExtensionMaxDepth=8` (with `UseTtExtension=true`)

- **Gate:** SPRT. **Pre-flight:** 538,355 (the base `UseTtExtension=true` alone: 502,408); change +7.2 %.
- **Hypothesis:** With the table extension on, nodes up to depth 8 may extend instead of 6.
- **Lever:** `TtExtensionMaxDepth`, default 6, range 0 to 12.
- **Dependency:** acts only with `UseTtExtension=true`; both sides run with it (`-BaseOptions`).
- **Command:**

```powershell
Invoke-SprtArm -Id P5_29_TtExtensionMaxDepth -BaseOptions "UseTtExtension=true" -ArmOptions "TtExtensionMaxDepth=8"
```

- **Status:** PENDING.

### P5.30 - `BnpDepth=4` (with `UseBadNoisyPruning=true`)

- **Gate:** SPRT. **Pre-flight:** 446,211 (the base `UseBadNoisyPruning=true` alone: 445,981); change +0.1 %.
- **Hypothesis:** With bad-noisy pruning on, only below depth 4 instead of 6.
- **Lever:** `BnpDepth`, default 6, range 2 to 10.
- **Dependency:** acts only with `UseBadNoisyPruning=true`; both sides run with it (`-BaseOptions`).
- **Command:**

```powershell
Invoke-SprtArm -Id P5_30_BnpDepth -BaseOptions "UseBadNoisyPruning=true" -ArmOptions "BnpDepth=4"
```

- **Status:** PENDING.

---

## 8. P6 - SPSA tuning (only after a lever has won)

Do **not** start SPSA before a lever has won its match, and tune one family at a time,
starting from the frozen binary plus the confirmed winner. The numeric levers are the
surface; the families:

- the late-move-reduction schedule: `LmrDivisor`, `LmrHistoryDivisor`, `LmrPvWeight`,
  `LmrKillerWeight`, `LmrMinDepth`, `LmrMinMoves`;
- the null move: `NmpBaseReduction`, `NmpDepthDivisor`, `NmpEvalDivisor`, `NmpEvalBonusMax`,
  `NmpMinDepth`;
- the pruning scales and gates: `RfpImprovingScale`, `RfpNotImprovingScale`, `RfpDamping`,
  `RazorMarginScale`, `FutilityScale`, `LmpScale` and their depth gates, the three SEE margins,
  `HistPruneMargin`;
- move ordering: `HistoryBonusScale`, `HistoryMalusScale`;
- ProbCut (`ProbCutMargin`, `ProbCutMinDepth`) only if `UseProbCut` won.

The ranges of `docs/LEVERS.md` are the SPSA bounds. A tuning result never re-opens the
frozen baseline: it is a new baseline with its own pins.

---

## 9. Results log

Add one row per completed arm and paste the result block of `matches\<ID>.result.txt`
under the table, or keep the files in `matches\` (git-ignored) and the rows here.

**Campaign run record** (fill in): start `__________`, end `__________`, binary sha256 `__________`,
commit `__________`, host state (power plan, other load) `__________`.

| ID | Arm | Date | Result (Elo, LLR, games) | Time losses | Verdict | Log |
| :--- | :--- | :--- | :--- | ---: | :--- | :--- |
| P1.01 | `UseProbCut=true` | | | | PENDING | `matches/P1_01_UseProbCut.log` |
| P1.02 | `UseTtExtension=true` | | | | PENDING | `matches/P1_02_UseTtExtension.log` |
| P1.03 | `UseQsCorrectionHistory=true` | | | | PENDING | `matches/P1_03_UseQsCorrectionHistory.log` |
| P1.04 | `UseBadNoisyPruning=true` | | | | PENDING | `matches/P1_04_UseBadNoisyPruning.log` |
| P1.05 | `LmrDivisor=225` | | | | PENDING | `matches/P1_05_LmrDivisor.log` |
| P1.06 | `LmrHistoryDivisor=2500` | | | | PENDING | `matches/P1_06_LmrHistoryDivisor.log` |
| P1.07 | `LmrPvWeight=600` | | | | PENDING | `matches/P1_07_LmrPvWeight.log` |
| P1.08 | `LmrKillerWeight=600` | | | | PENDING | `matches/P1_08_LmrKillerWeight.log` |
| P1.09 | `LmrMinMoves=2` | | | | PENDING | `matches/P1_09_LmrMinMoves.log` |
| P1.10 | `UseTerm1NotImproving=true` | | | | PENDING | `matches/P1_10_UseTerm1NotImproving.log` |
| P1.11 | `NmpBaseReduction=4` | | | | PENDING | `matches/P1_11_NmpBaseReduction.log` |
| P1.12 | `NmpEvalDivisor=140` | | | | PENDING | `matches/P1_12_NmpEvalDivisor.log` |
| P1.13 | `RfpNotImprovingScale=80` | | | | PENDING | `matches/P1_13_RfpNotImprovingScale.log` |
| P1.14 | `FutilityScale=85` | | | | PENDING | `matches/P1_14_FutilityScale.log` |
| P1.15 | `LmpScale=80` | | | | PENDING | `matches/P1_15_LmpScale.log` |
| P1.16 | `UseQsNarrowing=true` | | | | PENDING | `matches/P1_16_UseQsNarrowing.log` |
| P2.01 | `UseTtScoreEval=false` | | | | PENDING | `matches/P2_01_UseTtScoreEval.log` |
| P2.02 | `UseMicroRiders=false` | | | | PENDING | `matches/P2_02_UseMicroRiders.log` |
| P2.03 | `UseImprovingFlag=false` | | | | PENDING | `matches/P2_03_UseImprovingFlag.log` |
| P2.04 | `UseCutNode=false` | | | | PENDING | `matches/P2_04_UseCutNode.log` |
| P2.05 | `UseNegativeExtensions=false` | | | | PENDING | `matches/P2_05_UseNegativeExtensions.log` |
| P2.06 | `UseInternalIterativeReduction=false` | | | | PENDING | `matches/P2_06_UseInternalIterativeReduction.log` |
| P2.07 | `UseTrioHindsight=false` | | | | PENDING | `matches/P2_07_UseTrioHindsight.log` |
| P2.08 | `UseHistoryPruning=false` | | | | PENDING | `matches/P2_08_UseHistoryPruning.log` |
| P2.09 | `UseReSearchReduction=false` | | | | PENDING | `matches/P2_09_UseReSearchReduction.log` |
| P3.01 | `UseStableTimeManagement=false` | | | | PENDING | `matches/P3_01_UseStableTimeManagement.log` |
| P4.01 | `UseCorrectionHistory=false` | | | | PENDING | `matches/P4_01_UseCorrectionHistory.log` |
| P4.02 | `UseCheckAwareQuiescence=false` | | | | PENDING | `matches/P4_02_UseCheckAwareQuiescence.log` |
| P4.03 | `UseSeeApproxPrefilter=false` | | | | PENDING | `matches/P4_03_UseSeeApproxPrefilter.log` |
| P4.04 | `UseQsTtProbe=false` | | | | PENDING | `matches/P4_04_UseQsTtProbe.log` |
| P5.01 | `AspirationBase=33` | | | | PENDING | `matches/P5_01_AspirationBase.log` |
| P5.02 | `AspirationStartDepth=7` | | | | PENDING | `matches/P5_02_AspirationStartDepth.log` |
| P5.03 | `NmpDepthDivisor=3` | | | | PENDING | `matches/P5_03_NmpDepthDivisor.log` |
| P5.04 | `NmpEvalBonusMax=0` | | | | PENDING | `matches/P5_04_NmpEvalBonusMax.log` |
| P5.05 | `NmpMinDepth=4` | | | | PENDING | `matches/P5_05_NmpMinDepth.log` |
| P5.06 | `RfpFormulaMaxDepth=10` | | | | PENDING | `matches/P5_06_RfpFormulaMaxDepth.log` |
| P5.07 | `RfpImprovingScale=55` | | | | PENDING | `matches/P5_07_RfpImprovingScale.log` |
| P5.08 | `RfpDamping=2` | | | | PENDING | `matches/P5_08_RfpDamping.log` |
| P5.09 | `RazorMaxDepth=4` | | | | PENDING | `matches/P5_09_RazorMaxDepth.log` |
| P5.10 | `RazorMarginScale=80` | | | | PENDING | `matches/P5_10_RazorMarginScale.log` |
| P5.11 | `FutilityMaxDepth=9` | | | | PENDING | `matches/P5_11_FutilityMaxDepth.log` |
| P5.12 | `LmpMaxDepth=6` | | | | PENDING | `matches/P5_12_LmpMaxDepth.log` |
| P5.13 | `HistPruneMaxDepth=1` | | | | PENDING | `matches/P5_13_HistPruneMaxDepth.log` |
| P5.14 | `HistPruneMargin=1500` | | | | PENDING | `matches/P5_14_HistPruneMargin.log` |
| P5.15 | `SeePruningMaxDepth=10` | | | | PENDING | `matches/P5_15_SeePruningMaxDepth.log` |
| P5.16 | `SeeQuietMargin=-30` | | | | PENDING | `matches/P5_16_SeeQuietMargin.log` |
| P5.17 | `SeeNoisyMargin=-18` | | | | PENDING | `matches/P5_17_SeeNoisyMargin.log` |
| P5.18 | `SeeHistoryDivisor=192` | | | | PENDING | `matches/P5_18_SeeHistoryDivisor.log` |
| P5.19 | `NegExtHistoryThreshold=2000` | | | | PENDING | `matches/P5_19_NegExtHistoryThreshold.log` |
| P5.20 | `IirMinDepth=6` | | | | PENDING | `matches/P5_20_IirMinDepth.log` |
| P5.21 | `HindsightMargin=30` | | | | PENDING | `matches/P5_21_HindsightMargin.log` |
| P5.22 | `LmrMinDepth=2` | | | | PENDING | `matches/P5_22_LmrMinDepth.log` |
| P5.23 | `QsDeltaMargin=900` | | | | PENDING | `matches/P5_23_QsDeltaMargin.log` |
| P5.24 | `UseQsNarrowing=true QsRecaptureOnlyPly=4` | | | | PENDING | `matches/P5_24_QsRecaptureOnlyPly.log` |
| P5.25 | `HistoryBonusScale=130` | | | | PENDING | `matches/P5_25_HistoryBonusScale.log` |
| P5.26 | `HistoryMalusScale=130` | | | | PENDING | `matches/P5_26_HistoryMalusScale.log` |
| P5.27 | `UseProbCut=true ProbCutMargin=180` | | | | PENDING | `matches/P5_27_ProbCutMargin.log` |
| P5.28 | `UseProbCut=true ProbCutMinDepth=7` | | | | PENDING | `matches/P5_28_ProbCutMinDepth.log` |
| P5.29 | `UseTtExtension=true TtExtensionMaxDepth=8` | | | | PENDING | `matches/P5_29_TtExtensionMaxDepth.log` |
| P5.30 | `UseBadNoisyPruning=true BnpDepth=4` | | | | PENDING | `matches/P5_30_BnpDepth.log` |

---

## 10. Quick reference - lever -> arm

| Lever | Default | Range | Arm |
| :--- | ---: | :--- | :--- |
| `AspirationBase` | 50 | 10 to 100 | P5.01 (`AspirationBase=33`) |
| `AspirationStartDepth` | 5 | 1 to 10 | P5.02 (`AspirationStartDepth=7`) |
| `UseStableTimeManagement` | true | true / false | P3.01 (`UseStableTimeManagement=false`) |
| `NmpBaseReduction` | 3 | 1 to 6 | P1.11 (`NmpBaseReduction=4`) |
| `NmpDepthDivisor` | 4 | 1 to 8 | P5.03 (`NmpDepthDivisor=3`) |
| `NmpEvalDivisor` | 191 | 50 to 400 | P1.12 (`NmpEvalDivisor=140`) |
| `NmpEvalBonusMax` | 4 | 0 to 8 | P5.04 (`NmpEvalBonusMax=0`) |
| `NmpMinDepth` | 3 | 2 to 8 | P5.05 (`NmpMinDepth=4`) |
| `RfpFormulaMaxDepth` | 8 | 4 to 12 | P5.06 (`RfpFormulaMaxDepth=10`) |
| `RfpImprovingScale` | 65 | 10 to 200 | P5.07 (`RfpImprovingScale=55`) |
| `RfpNotImprovingScale` | 95 | 10 to 300 | P1.13 (`RfpNotImprovingScale=80`) |
| `RfpDamping` | 3 | 1 to 8 | P5.08 (`RfpDamping=2`) |
| `RazorMaxDepth` | 3 | 2 to 8 | P5.09 (`RazorMaxDepth=4`) |
| `RazorMarginScale` | 100 | 25 to 300 | P5.10 (`RazorMarginScale=80`) |
| `FutilityMaxDepth` | 7 | 4 to 10 | P5.11 (`FutilityMaxDepth=9`) |
| `FutilityScale` | 100 | 25 to 300 | P1.14 (`FutilityScale=85`) |
| `LmpMaxDepth` | 4 | 2 to 8 | P5.12 (`LmpMaxDepth=6`) |
| `LmpScale` | 100 | 25 to 300 | P1.15 (`LmpScale=80`) |
| `UseHistoryPruning` | true | true / false | P2.08 (`UseHistoryPruning=false`) |
| `HistPruneMaxDepth` | 3 | 1 to 8 | P5.13 (`HistPruneMaxDepth=1`) |
| `HistPruneMargin` | 2000 | 800 to 4000 | P5.14 (`HistPruneMargin=1500`) |
| `SeePruningMaxDepth` | 8 | 2 to 10 | P5.15 (`SeePruningMaxDepth=10`) |
| `SeeQuietMargin` | -43 | -120 to 0 | P5.16 (`SeeQuietMargin=-30`) |
| `SeeNoisyMargin` | -26 | -100 to 0 | P5.17 (`SeeNoisyMargin=-18`) |
| `SeeHistoryDivisor` | 128 | 32 to 512 | P5.18 (`SeeHistoryDivisor=192`) |
| `UseNegativeExtensions` | true | true / false | P2.05 (`UseNegativeExtensions=false`) |
| `NegExtHistoryThreshold` | 3000 | 500 to 8000 | P5.19 (`NegExtHistoryThreshold=2000`) |
| `UseInternalIterativeReduction` | true | true / false | P2.06 (`UseInternalIterativeReduction=false`) |
| `IirMinDepth` | 4 | 2 to 10 | P5.20 (`IirMinDepth=6`) |
| `UseTrioHindsight` | true | true / false | P2.07 (`UseTrioHindsight=false`) |
| `HindsightMargin` | 62 | 0 to 200 | P5.21 (`HindsightMargin=30`) |
| `LmrDivisor` | 175 | 100 to 300 | P1.05 (`LmrDivisor=225`) |
| `LmrMinDepth` | 3 | 2 to 8 | P5.22 (`LmrMinDepth=2`) |
| `LmrMinMoves` | 3 | 1 to 8 | P1.09 (`LmrMinMoves=2`) |
| `LmrHistoryDivisor` | 3419 | 1000 to 8000 | P1.06 (`LmrHistoryDivisor=2500`) |
| `LmrPvWeight` | 1024 | 0 to 2048 | P1.07 (`LmrPvWeight=600`) |
| `LmrKillerWeight` | 1024 | 0 to 2048 | P1.08 (`LmrKillerWeight=600`) |
| `UseTerm1NotImproving` | false | true / false | P1.10 (`UseTerm1NotImproving=true`) |
| `UseCutNode` | true | true / false | P2.04 (`UseCutNode=false`) |
| `UseImprovingFlag` | true | true / false | P2.03 (`UseImprovingFlag=false`) |
| `UseReSearchReduction` | true | true / false | P2.09 (`UseReSearchReduction=false`) |
| `QsDeltaMargin` | 1100 | 700 to 1500 | P5.23 (`QsDeltaMargin=900`) |
| `UseQsNarrowing` | false | true / false | P1.16 (`UseQsNarrowing=true`) |
| `QsRecaptureOnlyPly` | 8 | 0 to 16 | P5.24 (`QsRecaptureOnlyPly=4`) |
| `UseMicroRiders` | true | true / false | P2.02 (`UseMicroRiders=false`) |
| `UseCheckAwareQuiescence` | true | true / false | P4.02 (`UseCheckAwareQuiescence=false`) |
| `UseSeeApproxPrefilter` | true | true / false | P4.03 (`UseSeeApproxPrefilter=false`) |
| `UseQsTtProbe` | true | true / false | P4.04 (`UseQsTtProbe=false`) |
| `UseCorrectionHistory` | true | true / false | P4.01 (`UseCorrectionHistory=false`) |
| `UseQsCorrectionHistory` | false | true / false | P1.03 (`UseQsCorrectionHistory=true`) |
| `UseTtScoreEval` | true | true / false | P2.01 (`UseTtScoreEval=false`) |
| `HistoryBonusScale` | 100 | 25 to 300 | P5.25 (`HistoryBonusScale=130`) |
| `HistoryMalusScale` | 100 | 25 to 300 | P5.26 (`HistoryMalusScale=130`) |
| `UseProbCut` | false | true / false | P1.01 (`UseProbCut=true`) |
| `ProbCutMargin` | 235 | 100 to 400 | P5.27 (`ProbCutMargin=180`) |
| `ProbCutMinDepth` | 5 | 4 to 10 | P5.28 (`ProbCutMinDepth=7`) |
| `UseTtExtension` | false | true / false | P1.02 (`UseTtExtension=true`) |
| `TtExtensionMaxDepth` | 6 | 0 to 12 | P5.29 (`TtExtensionMaxDepth=8`) |
| `UseBadNoisyPruning` | false | true / false | P1.04 (`UseBadNoisyPruning=true`) |
| `BnpDepth` | 6 | 2 to 10 | P5.30 (`BnpDepth=4`) |

---

_Test plan written 2026-11-19 for the frozen source named in the snapshot. Levers:
`docs/LEVERS.md`. Decisions: `docs/decisions/ADR-0014-the-sixty-levers.md`. Helpers:
`tools/sprt_arm.ps1`, `tools/lever_campaign.ps1`. Predecessor template:
`audit-tests.md` of PawnForgeCS. **No code change may be made during the campaign: a
code change invalidates every verdict.**_
