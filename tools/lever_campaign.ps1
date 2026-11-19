<#
.SYNOPSIS
  Runs the lever test queue of docs/audit-tests-KB.md, one arm after the other.

.DESCRIPTION
  Generated together with the document; the arm table below is the same data as
  its queue tables (id, option string, base options, games, and the fixed-depth
  pre-flight totals measured on the frozen commit).

      $env:KB_ENGINE = "<the campaign binary>"
      .\tools\lever_campaign.ps1 -Group P1           # all arms of one priority
      .\tools\lever_campaign.ps1 -Only P1.03         # one arm
      .\tools\lever_campaign.ps1 -All                # the whole queue (about 34 hours)

  Before each arm the script runs the pre-flight: `bench 12` with the arm must
  print the recorded total, and the base alone must print its recorded total. A
  mismatch means a different binary or a lever that is not wired, and the arm is
  not played. An arm whose matches\<id>.result.txt exists is skipped (resume),
  unless -Force is given. Nothing is baked: results go to matches\ and are copied
  into the results log of the document by hand.

  -Games and -Tc override the standard (400 games, 10+0.1) for a smoke test only.
#>
param(
    [string]$Group = "",
    [string]$Only = "",
    [switch]$All,
    [switch]$Force,
    [switch]$SkipPreflight,
    [int]$Games = 0,
    [string]$Tc = "",
    [string]$OutDir = "matches"
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\sprt_arm.ps1"

if (-not $env:KB_ENGINE -or -not (Test-Path $env:KB_ENGINE)) { throw "set $env:KB_ENGINE to the campaign binary" }
if (-not ($Group -or $Only -or $All)) { throw "choose -Group P1..P5, -Only P1.03 or -All" }

# The frozen default total: every pre-flight below is relative to it.
$DefaultBench = 456308

$Arms = @(
    @{ Id = "P1.01"; File = "P1_01_UseProbCut"; Arm = "UseProbCut=true"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 442190 },
    @{ Id = "P1.02"; File = "P1_02_UseTtExtension"; Arm = "UseTtExtension=true"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 502408 },
    @{ Id = "P1.03"; File = "P1_03_UseQsCorrectionHistory"; Arm = "UseQsCorrectionHistory=true"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 536219 },
    @{ Id = "P1.04"; File = "P1_04_UseBadNoisyPruning"; Arm = "UseBadNoisyPruning=true"; Base = ""; Games = 800; BaseBench = 456308; ArmBench = 445981 },
    @{ Id = "P1.05"; File = "P1_05_LmrDivisor"; Arm = "LmrDivisor=225"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 573915 },
    @{ Id = "P1.06"; File = "P1_06_LmrHistoryDivisor"; Arm = "LmrHistoryDivisor=2500"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 442829 },
    @{ Id = "P1.07"; File = "P1_07_LmrPvWeight"; Arm = "LmrPvWeight=600"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 413497 },
    @{ Id = "P1.08"; File = "P1_08_LmrKillerWeight"; Arm = "LmrKillerWeight=600"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 518382 },
    @{ Id = "P1.09"; File = "P1_09_LmrMinMoves"; Arm = "LmrMinMoves=2"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 370559 },
    @{ Id = "P1.10"; File = "P1_10_UseTerm1NotImproving"; Arm = "UseTerm1NotImproving=true"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 330643 },
    @{ Id = "P1.11"; File = "P1_11_NmpBaseReduction"; Arm = "NmpBaseReduction=4"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 380463 },
    @{ Id = "P1.12"; File = "P1_12_NmpEvalDivisor"; Arm = "NmpEvalDivisor=140"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 388793 },
    @{ Id = "P1.13"; File = "P1_13_RfpNotImprovingScale"; Arm = "RfpNotImprovingScale=80"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 440620 },
    @{ Id = "P1.14"; File = "P1_14_FutilityScale"; Arm = "FutilityScale=85"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 532647 },
    @{ Id = "P1.15"; File = "P1_15_LmpScale"; Arm = "LmpScale=80"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 442123 },
    @{ Id = "P1.16"; File = "P1_16_UseQsNarrowing"; Arm = "UseQsNarrowing=true"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 428894 },
    @{ Id = "P2.01"; File = "P2_01_UseTtScoreEval"; Arm = "UseTtScoreEval=false"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 388414 },
    @{ Id = "P2.02"; File = "P2_02_UseMicroRiders"; Arm = "UseMicroRiders=false"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 437184 },
    @{ Id = "P2.03"; File = "P2_03_UseImprovingFlag"; Arm = "UseImprovingFlag=false"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 603732 },
    @{ Id = "P2.04"; File = "P2_04_UseCutNode"; Arm = "UseCutNode=false"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 436287 },
    @{ Id = "P2.05"; File = "P2_05_UseNegativeExtensions"; Arm = "UseNegativeExtensions=false"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 439126 },
    @{ Id = "P2.06"; File = "P2_06_UseInternalIterativeReduction"; Arm = "UseInternalIterativeReduction=false"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 480101 },
    @{ Id = "P2.07"; File = "P2_07_UseTrioHindsight"; Arm = "UseTrioHindsight=false"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 443994 },
    @{ Id = "P2.08"; File = "P2_08_UseHistoryPruning"; Arm = "UseHistoryPruning=false"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 537894 },
    @{ Id = "P2.09"; File = "P2_09_UseReSearchReduction"; Arm = "UseReSearchReduction=false"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 553573 },
    @{ Id = "P3.01"; File = "P3_01_UseStableTimeManagement"; Arm = "UseStableTimeManagement=false"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 456308 },
    @{ Id = "P4.01"; File = "P4_01_UseCorrectionHistory"; Arm = "UseCorrectionHistory=false"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 484853 },
    @{ Id = "P4.02"; File = "P4_02_UseCheckAwareQuiescence"; Arm = "UseCheckAwareQuiescence=false"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 422703 },
    @{ Id = "P4.03"; File = "P4_03_UseSeeApproxPrefilter"; Arm = "UseSeeApproxPrefilter=false"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 340948 },
    @{ Id = "P4.04"; File = "P4_04_UseQsTtProbe"; Arm = "UseQsTtProbe=false"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 449325 },
    @{ Id = "P5.01"; File = "P5_01_AspirationBase"; Arm = "AspirationBase=33"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 647408 },
    @{ Id = "P5.02"; File = "P5_02_AspirationStartDepth"; Arm = "AspirationStartDepth=7"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 368657 },
    @{ Id = "P5.03"; File = "P5_03_NmpDepthDivisor"; Arm = "NmpDepthDivisor=3"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 450679 },
    @{ Id = "P5.04"; File = "P5_04_NmpEvalBonusMax"; Arm = "NmpEvalBonusMax=0"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 444762 },
    @{ Id = "P5.05"; File = "P5_05_NmpMinDepth"; Arm = "NmpMinDepth=4"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 464233 },
    @{ Id = "P5.06"; File = "P5_06_RfpFormulaMaxDepth"; Arm = "RfpFormulaMaxDepth=10"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 455530 },
    @{ Id = "P5.07"; File = "P5_07_RfpImprovingScale"; Arm = "RfpImprovingScale=55"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 515700 },
    @{ Id = "P5.08"; File = "P5_08_RfpDamping"; Arm = "RfpDamping=2"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 438965 },
    @{ Id = "P5.09"; File = "P5_09_RazorMaxDepth"; Arm = "RazorMaxDepth=4"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 457019 },
    @{ Id = "P5.10"; File = "P5_10_RazorMarginScale"; Arm = "RazorMarginScale=80"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 563540 },
    @{ Id = "P5.11"; File = "P5_11_FutilityMaxDepth"; Arm = "FutilityMaxDepth=9"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 442770 },
    @{ Id = "P5.12"; File = "P5_12_LmpMaxDepth"; Arm = "LmpMaxDepth=6"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 438259 },
    @{ Id = "P5.13"; File = "P5_13_HistPruneMaxDepth"; Arm = "HistPruneMaxDepth=1"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 422845 },
    @{ Id = "P5.14"; File = "P5_14_HistPruneMargin"; Arm = "HistPruneMargin=1500"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 425556 },
    @{ Id = "P5.15"; File = "P5_15_SeePruningMaxDepth"; Arm = "SeePruningMaxDepth=10"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 451803 },
    @{ Id = "P5.16"; File = "P5_16_SeeQuietMargin"; Arm = "SeeQuietMargin=-30"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 406602 },
    @{ Id = "P5.17"; File = "P5_17_SeeNoisyMargin"; Arm = "SeeNoisyMargin=-18"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 467679 },
    @{ Id = "P5.18"; File = "P5_18_SeeHistoryDivisor"; Arm = "SeeHistoryDivisor=192"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 485273 },
    @{ Id = "P5.19"; File = "P5_19_NegExtHistoryThreshold"; Arm = "NegExtHistoryThreshold=2000"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 475746 },
    @{ Id = "P5.20"; File = "P5_20_IirMinDepth"; Arm = "IirMinDepth=6"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 392859 },
    @{ Id = "P5.21"; File = "P5_21_HindsightMargin"; Arm = "HindsightMargin=30"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 510289 },
    @{ Id = "P5.22"; File = "P5_22_LmrMinDepth"; Arm = "LmrMinDepth=2"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 460888 },
    @{ Id = "P5.23"; File = "P5_23_QsDeltaMargin"; Arm = "QsDeltaMargin=900"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 453686 },
    @{ Id = "P5.24"; File = "P5_24_QsRecaptureOnlyPly"; Arm = "QsRecaptureOnlyPly=4"; Base = "UseQsNarrowing=true"; Games = 400; BaseBench = 428894; ArmBench = 437102 },
    @{ Id = "P5.25"; File = "P5_25_HistoryBonusScale"; Arm = "HistoryBonusScale=130"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 434036 },
    @{ Id = "P5.26"; File = "P5_26_HistoryMalusScale"; Arm = "HistoryMalusScale=130"; Base = ""; Games = 400; BaseBench = 456308; ArmBench = 425788 },
    @{ Id = "P5.27"; File = "P5_27_ProbCutMargin"; Arm = "ProbCutMargin=180"; Base = "UseProbCut=true"; Games = 400; BaseBench = 442190; ArmBench = 404945 },
    @{ Id = "P5.28"; File = "P5_28_ProbCutMinDepth"; Arm = "ProbCutMinDepth=7"; Base = "UseProbCut=true"; Games = 400; BaseBench = 442190; ArmBench = 459622 },
    @{ Id = "P5.29"; File = "P5_29_TtExtensionMaxDepth"; Arm = "TtExtensionMaxDepth=8"; Base = "UseTtExtension=true"; Games = 400; BaseBench = 502408; ArmBench = 538355 },
    @{ Id = "P5.30"; File = "P5_30_BnpDepth"; Arm = "BnpDepth=4"; Base = "UseBadNoisyPruning=true"; Games = 400; BaseBench = 445981; ArmBench = 446211 }
)

$selected = $Arms | Where-Object {
    $All -or ($Group -and $_.Id.StartsWith("$Group.")) -or ($Only -and $_.Id -eq $Only)
}
if (-not $selected) { throw "no arm matches the selection" }

$handshake = ("uci`nquit`n" | & $env:KB_ENGINE | Where-Object { $_ -match "^option name " }).Count
if ($handshake -ne 70) { throw "the handshake lists $handshake options, expected 70 (10 public + 60 levers)" }
if (-not $SkipPreflight) {
    $actual = Get-ArmBench -ArmOptions "UseStableTimeManagement=true"
    if ($actual -ne $DefaultBench) { throw "default bench 12 is $actual, the frozen total is ${DefaultBench}: wrong binary" }
}

foreach ($arm in $selected) {
    $result = Join-Path $OutDir ($arm.File + ".result.txt")
    if ((Test-Path $result) -and -not $Force) {
        Write-Host "[$($arm.Id)] skipped: $result exists"
        continue
    }
    if (-not $SkipPreflight) {
        $base = if ($arm.Base) { Get-ArmBench -ArmOptions $arm.Base } else { $DefaultBench }
        $withArm = Get-ArmBench -ArmOptions $arm.Arm -BaseOptions $arm.Base
        if ($base -ne $arm.BaseBench -or $withArm -ne $arm.ArmBench) {
            Write-Warning "[$($arm.Id)] pre-flight mismatch: base $base (expected $($arm.BaseBench)), arm $withArm (expected $($arm.ArmBench)); not played"
            continue
        }
        if ($arm.ArmBench -eq $arm.BaseBench -and $arm.Id -ne "P3.01") {
            Write-Warning "[$($arm.Id)] the arm prints the base total: the lever is not wired; not played"
            continue
        }
    }
    $gamesToPlay = if ($Games -gt 0) { $Games } else { $arm.Games }
    $arguments = @{ Id = $arm.File; ArmOptions = $arm.Arm; BaseOptions = $arm.Base; Games = $gamesToPlay; OutDir = $OutDir }
    if ($Tc) { $arguments.Tc = $Tc }
    Write-Host "[$($arm.Id)] $($arm.Base) $($arm.Arm): $gamesToPlay games"
    Invoke-SprtArm @arguments
}
