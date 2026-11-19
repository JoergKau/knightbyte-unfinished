<#
.SYNOPSIS
  Builds a profile-guided (PGO) release binary of the engine on Windows.

.DESCRIPTION
  1. builds an instrumented binary (-Cprofile-generate),
  2. runs the deterministic bench workloads to collect profiles,
  3. merges them with llvm-profdata,
  4. rebuilds with -Cprofile-use.

  The profile describes ONE host and ONE build configuration: generate it on
  the machine (and with the target-cpu / feature set) the binary is built for,
  and never reuse or check in the .profdata files. PGO must not change the
  search: the script verifies the four bench node pins on the result and fails
  if any of them moves.

  Requires the llvm-tools component:  rustup component add llvm-tools-preview

.PARAMETER WorkDir
  Scratch directory for the instrumented build, raw profiles and the merged
  profile (default: pgo-work under the repository, git-ignored by the caller).

.PARAMETER Output
  Where the final PGO binary is copied (default: target\pgo\knightbyte.exe).

.PARAMETER TargetCpu
  Optional `-Ctarget-cpu` for a dedicated variant (for example `x86-64-v3` or
  `x86-64-v4`). Empty builds the universal executable, whose SIMD backend is
  chosen at run time (ADR-0001). The instrumented and the profile-use builds
  always use the same value.

.PARAMETER TargetFeatures
  Optional `-Ctarget-feature` list (for example `+avx512vnni`), same rules.

.EXAMPLE
  powershell -File tools\pgo_build.ps1
#>
param(
    [string]$WorkDir = "pgo-work",
    [string]$Output = "target\pgo\knightbyte.exe",
    [string]$TargetCpu = "",
    [string]$TargetFeatures = ""
)

$ErrorActionPreference = "Stop"
$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repo

# Node identity pins of the four-position bench (depths 6, 12, 16, 18).
$pins = @{ 6 = 15528; 12 = 456308; 16 = 2815799; 18 = 6892874 }

$sysroot = (& rustc --print sysroot).Trim()
$host_triple = (& rustc -vV | Select-String "^host:").ToString().Split(" ")[1]
$profdata = Join-Path $sysroot "lib\rustlib\$host_triple\bin\llvm-profdata.exe"
if (-not (Test-Path $profdata)) {
    throw "llvm-profdata not found; run: rustup component add llvm-tools-preview"
}

# The C runtime is linked statically: a plain Windows 10 machine has no
# VCRUNTIME140.dll unless the Visual C++ redistributable was installed, and the
# engine must start on a tester's machine without it. (RUSTFLAGS replaces the
# flags of .cargo/config.toml, so the flag is repeated here.)
$cpuFlags = "-Ctarget-feature=+crt-static"
if ($TargetCpu -ne "") { $cpuFlags += " -Ctarget-cpu=$TargetCpu" }
if ($TargetFeatures -ne "") { $cpuFlags += " -Ctarget-feature=$TargetFeatures" }
$cpuFlags = $cpuFlags.Trim()
Write-Host "rustflags: $cpuFlags"

$work = Join-Path $repo $WorkDir
$raw = Join-Path $work "raw"
$merged = Join-Path $work "merged.profdata"
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $raw | Out-Null

Write-Host "== 1/4 instrumented build"
$env:RUSTFLAGS = "$cpuFlags -Cprofile-generate=$raw".Trim()
& cargo build --release --target-dir (Join-Path $work "gen")
if ($LASTEXITCODE -ne 0) { throw "instrumented build failed" }
Remove-Item Env:RUSTFLAGS

Write-Host "== 2/4 collecting profiles"
$instrumented = Join-Path $work "gen\release\knightbyte.exe"
foreach ($depth in 12, 16) {
    & $instrumented bench $depth | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "profile run failed at depth $depth" }
}

Write-Host "== 3/4 merging"
& $profdata merge -o $merged $raw
if ($LASTEXITCODE -ne 0) { throw "llvm-profdata merge failed" }

Write-Host "== 4/4 profile-use build"
$env:RUSTFLAGS = "$cpuFlags -Cprofile-use=$merged".Trim()
& cargo build --release --target-dir (Join-Path $work "use")
if ($LASTEXITCODE -ne 0) { throw "profile-use build failed" }
Remove-Item Env:RUSTFLAGS

$binary = Join-Path $work "use\release\knightbyte.exe"
Write-Host "== verifying the node pins"
foreach ($depth in $pins.Keys | Sort-Object) {
    $line = & $binary bench $depth | Select-String "^Nodes:"
    $nodes = [int64]($line.ToString().Split(":")[1].Trim())
    if ($nodes -ne $pins[$depth]) {
        throw "PGO build moved the bench-$depth pin: $nodes (expected $($pins[$depth])) - do not ship"
    }
}

$destination = if ([System.IO.Path]::IsPathRooted($Output)) { $Output } else { Join-Path $repo $Output }
New-Item -ItemType Directory -Force (Split-Path $destination) | Out-Null
Copy-Item $binary $destination -Force
Write-Host "PGO binary: $destination (pins exact)"
