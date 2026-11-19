<#
.SYNOPSIS
  Helpers for the lever test campaign (docs/audit-tests-KB.md).

.DESCRIPTION
  Dot-source this file once per PowerShell session:

      . .\tools\sprt_arm.ps1
      $env:KB_ENGINE = "C:\path\to\knightbyte-v4-vnni.exe"

  then run one arm per call:

      Invoke-SprtArm -Id P1_01_UseProbCut -ArmOptions "UseProbCut=true"

  Invoke-SprtArm plays the arm ("ON": the lever options set) against the same
  binary with defaults ("OFF"), one lever per match, with the campaign standard:
  400 games, 10+0.1, one thread, 256 MB, UHO openings in random order, seed
  1234, SPRT 0 -> 5 with bounds of about +/-2 LLR, Move Overhead 10 on both
  sides. It writes matches\<Id>.log (the fastchess output), matches\<Id>.pgn and
  matches\<Id>.result.txt (the verbatim result block and the cleanliness check),
  kills fastchess after -HardMinutes, and prints a one-line summary.

  A knob that only acts with its switch (ProbCutMargin needs UseProbCut) is run
  with -BaseOptions "UseProbCut=true": both sides get the base options and only
  the knob differs, so the match still changes one variable.

  Get-ArmBench prints the fixed-depth pre-flight total of an arm; it must differ
  from the total of the base alone, otherwise the lever is not wired.
#>

# SPRT bounds. fastchess accepts alpha and beta; with alpha = beta = 0.12 the
# log-likelihood bounds are ln(0.88 / 0.12) = +/-1.99, the +/-2 of the decision
# rule in docs/audit-tests-KB.md.
$script:SprtAlpha = 0.12
$script:SprtBeta = 0.12

function Get-FastChess {
    if ($env:KB_FASTCHESS) { return $env:KB_FASTCHESS }
    return "C:\dev\fastchess-windows-x86-64\fastchess.exe"
}

function Get-OpeningBook {
    if ($env:KB_BOOK) { return $env:KB_BOOK }
    return (Join-Path (Resolve-Path (Join-Path $PSScriptRoot "..")) "books\UHO_Lichess_4852_v1.epd")
}

# Quotes one command-line argument for Windows PowerShell 5.1, which has no
# ProcessStartInfo.ArgumentList.
function ConvertTo-CommandLineArgument([string]$Value) {
    if ($Value -notmatch '[\s"]') { return $Value }
    return '"' + ($Value -replace '"', '\"') + '"'
}

function Split-ArmOptions([string]$ArmOptions) {
    # "UseProbCut=true ProbCutMargin=180" -> @("option.UseProbCut=true", ...)
    @($ArmOptions -split '\s+' | Where-Object { $_ -ne "" } | ForEach-Object {
        if ($_ -notmatch '^[A-Za-z0-9]+=\S+$') { throw "bad arm option '$_' (expected Name=value)" }
        "option.$_"
    })
}

function Get-ArmBench {
    param(
        [Parameter(Mandatory)][string]$ArmOptions,
        [string]$BaseOptions = "",
        [int]$Depth = 12,
        [string]$Engine = $env:KB_ENGINE
    )
    if (-not $Engine) { throw "set `$env:KB_ENGINE to the campaign binary" }
    # @() keeps a single option an array: splatting a scalar string is not the
    # same as splatting a one-element array.
    $overrides = @("$BaseOptions $ArmOptions" -split '\s+' | Where-Object { $_ -ne "" } | ForEach-Object { "param:$_" })
    $line = & $Engine @overrides "bench $Depth" | Select-String "^Nodes:"
    return [int64]($line.ToString().Split(":")[1].Trim())
}

function Invoke-SprtArm {
    param(
        [Parameter(Mandatory)][string]$Id,
        [Parameter(Mandatory)][string]$ArmOptions,
        [string]$BaseOptions = "",
        [string]$Engine = $env:KB_ENGINE,
        [int]$Games = 400,
        [string]$Tc = "10+0.1",
        [int]$Concurrency = 6,
        [int]$Hash = 256,
        [int]$Overhead = 10,
        [int]$Seed = 1234,
        [string]$OutDir = "matches",
        [int]$HardMinutes = 360,
        [switch]$NoSprt
    )
    if (-not $Engine -or -not (Test-Path $Engine)) { throw "set `$env:KB_ENGINE to the campaign binary" }
    $book = Get-OpeningBook
    if (-not (Test-Path $book)) { throw "opening book not found: $book" }
    # An absolute folder, and fastchess runs inside it: it drops a config.json
    # into its working directory, which must not be the repository.
    $OutDir = (New-Item -ItemType Directory -Force $OutDir).FullName
    $log = Join-Path $OutDir "$Id.log"
    $pgn = Join-Path $OutDir "$Id.pgn"
    $result = Join-Path $OutDir "$Id.result.txt"
    Remove-Item $log, $pgn, $result -ErrorAction SilentlyContinue

    $common = @("option.Hash=$Hash", "option.Move Overhead=$Overhead")
    $base = if ($BaseOptions.Trim() -ne "") { Split-ArmOptions $BaseOptions } else { @() }
    $arguments = @("-engine", "name=ON", "cmd=$Engine") + $common + $base + (Split-ArmOptions $ArmOptions) +
        @("-engine", "name=OFF", "cmd=$Engine") + $common + $base +
        @("-each", "tc=$Tc", "-rounds", [string]($Games / 2), "-games", "2", "-repeat",
          "-concurrency", [string]$Concurrency,
          "-openings", "file=$book", "format=epd", "order=random", "-srand", [string]$Seed,
          "-resign", "movecount=3", "score=600", "-draw", "movenumber=34", "movecount=8", "score=20",
          "-pgnout", "file=$pgn", "-report", "penta=true", "-ratinginterval", "20")
    if (-not $NoSprt) {
        $arguments += @("-sprt", "elo0=0", "elo1=5", "alpha=$script:SprtAlpha", "beta=$script:SprtBeta",
            "model=normalized")
    }

    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = Get-FastChess
    $info.Arguments = ($arguments | ForEach-Object { ConvertTo-CommandLineArgument $_ }) -join " "
    $info.WorkingDirectory = $OutDir
    $info.UseShellExecute = $false
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::Start($info)
    $outTask = $process.StandardOutput.ReadToEndAsync()
    $errTask = $process.StandardError.ReadToEndAsync()
    $finished = $process.WaitForExit($HardMinutes * 60 * 1000)
    if (-not $finished) {
        $process.Kill()
        $process.WaitForExit()
        Get-Process -Name (Split-Path $Engine -LeafBase) -ErrorAction SilentlyContinue | Stop-Process -Force
    }
    $text = $outTask.Result + $errTask.Result
    Set-Content $log $text -Encoding utf8

    # Cleanliness: a match is only evidence if it was clean.
    $lines = $text -split "`r?`n"
    $timeLosses = ($lines | Select-String -Pattern "loses on time|time forfeit" -CaseSensitive:$false).Count
    $illegal = ($lines | Select-String -Pattern "illegal move|disconnect|crashed|stalls" -CaseSensitive:$false |
        Where-Object { $_.Line -notmatch "Crashed: 0" }).Count
    $score = ($lines | Select-String -Pattern "^Games:" | Select-Object -Last 1)
    $elo = ($lines | Select-String -Pattern "^Elo:" | Select-Object -Last 1)
    $los = ($lines | Select-String -Pattern "^LOS:" | Select-Object -Last 1)
    $llr = ($lines | Select-String -Pattern "LLR" | Select-Object -Last 1)
    $block = @(
        "ID        : $Id",
        "Base      : $BaseOptions",
        "Arm       : $ArmOptions",
        "Engine    : $Engine",
        "Finished  : $finished (exit code $(if ($finished) { $process.ExitCode } else { 'killed' }))",
        "Score     : $score",
        "Elo       : $elo",
        "LOS       : $los",
        "SPRT      : $llr",
        "Time loss : $timeLosses",
        "Abnormal  : $illegal (illegal move, disconnect, crash, stall)"
    ) -join "`n"
    Set-Content $result $block -Encoding utf8
    Write-Host $block
    if ($timeLosses -gt 0 -or $illegal -gt 0 -or -not $finished) {
        Write-Warning "$Id was NOT clean: re-run it (see docs/audit-tests-KB.md section 0.1) before using the result."
    }
}
