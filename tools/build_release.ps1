<#
.SYNOPSIS
  Builds, verifies and packages the Windows x64 release artifacts.

.DESCRIPTION
  For every variant of ADR-0011 the script
    1. builds a profile-guided executable with tools\pgo_build.ps1 (its own
       profile, the C runtime linked statically; the script verifies the four
       bench node pins),
    2. runs the executable: `uci` must answer `uciok`, `bench 6` must report
       the pinned node count, and a forced-ISA run must report the same count,
    3. checks the import table: only Windows system DLLs, and no Visual C++
       runtime (a plain Windows 10 machine does not have it),
    4. records the SHA-256 of the file.
  It then writes SHA256SUMS.txt and manifest.json and packs the executables with
  README.md, LICENSE and THIRD-PARTY-NOTICES.md into one zip. Before the first
  build it runs the lever effect gate (every advertised lever must change the
  search).

  Profiles describe one host: run this on the machine whose CPU supports the
  most capable variant (the v4-vnni build is profiled by running it).

.PARAMETER OutDir
  Output directory (default: dist, git-ignored).

.PARAMETER Variants
  Variants to build (default: v2, v3, v4-vnni).

.EXAMPLE
  powershell -File tools\build_release.ps1
#>
param(
    [string]$OutDir = "dist",
    [string[]]$Variants = @("v2", "v3", "v4-vnni")
)

$ErrorActionPreference = "Stop"
$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repo

# Variant table: the executable's compile-time baseline (ADR-0011).
$table = @{
    "v2"      = @{ Cpu = "x86-64-v2"; Features = "" }
    "v3"      = @{ Cpu = "x86-64-v3"; Features = "" }
    "v4-vnni" = @{ Cpu = "x86-64-v4"; Features = "+avx512vnni" }
}

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
$commit = (& git rev-parse --short HEAD).Trim()
$dirty = [bool](& git status --porcelain --untracked-files=no)
$rustc = (& rustc --version).Trim()
$stem = "knightbyte-$version-windows-x64"

$out = Join-Path $repo $OutDir
$staging = Join-Path $out $stem
Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $staging | Out-Null

$allowedDlls = @(
    "kernel32.dll", "ntdll.dll", "bcryptprimitives.dll",
    "api-ms-win-core-synch-l1-2-0.dll"
)

function Count-Nodes([string]$exe, [hashtable]$environment) {
    foreach ($name in $environment.Keys) { Set-Item "Env:$name" $environment[$name] }
    try {
        $line = & $exe bench 6 | Select-String "^Nodes:"
    } finally {
        foreach ($name in $environment.Keys) { Remove-Item "Env:$name" -ErrorAction SilentlyContinue }
    }
    return [int64]($line.ToString().Split(":")[1].Trim())
}

# Gate before any artifact is built: every advertised lever must change the
# search (ADR-0014). It takes seconds and catches a lever that was advertised
# but wired to nothing.
Write-Host "=== lever effect gate"
& cargo test --release --test lever_effect
if ($LASTEXITCODE -ne 0) { throw "the lever effect gate failed" }

$entries = @()
foreach ($variant in $Variants) {
    if (-not $table.ContainsKey($variant)) { throw "unknown variant: $variant" }
    $spec = $table[$variant]
    Write-Host "=== variant $variant (target-cpu $($spec.Cpu) $($spec.Features))"

    $exe = Join-Path $staging "knightbyte-$variant.exe"
    $arguments = @(
        "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", (Join-Path $PSScriptRoot "pgo_build.ps1"),
        "-WorkDir", "pgo-work-$variant", "-Output", $exe, "-TargetCpu", $spec.Cpu
    )
    if ($spec.Features -ne "") { $arguments += @("-TargetFeatures", $spec.Features) }
    & powershell @arguments
    if ($LASTEXITCODE -ne 0) { throw "PGO build of $variant failed" }

    # 2. behaviour checks on the packaged file itself
    $uci = ("uci`nquit`n" | & $exe) -join "`n"
    if ($uci -notmatch "uciok") { throw "$variant did not answer uciok" }
    $nodes = Count-Nodes $exe @{}
    if ($nodes -ne 15528) { throw "$variant bench 6 reported $nodes nodes" }
    # The slowest backend must search the identical tree (ADR-0001 gate).
    $scalar = Count-Nodes $exe @{ KNIGHTBYTE_FORCE_ISA = "scalar" }
    if ($scalar -ne $nodes) { throw "$variant forced scalar reported $scalar nodes" }

    # 3. import table
    $imports = & python (Join-Path $PSScriptRoot "pe_imports.py") $exe
    $dlls = $imports | Where-Object { $_ -match '^\S' } | ForEach-Object { ($_ -split ' ')[0].ToLower() }
    $unexpected = $dlls | Where-Object { $_ -notin $allowedDlls }
    if ($unexpected) { throw "$variant imports unexpected DLLs: $($unexpected -join ', ')" }

    $hash = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLower()
    $entries += [ordered]@{
        variant = $variant
        file = "knightbyte-$variant.exe"
        target_cpu = $spec.Cpu
        target_features = $spec.Features
        bytes = (Get-Item $exe).Length
        sha256 = $hash
        bench6_nodes = $nodes
    }
    Write-Host "  ok: $nodes nodes, forced scalar identical, imports clean, sha256 $hash"
}

# 4. checksums, manifest, documentation, archive
$entries | ForEach-Object { "$($_.sha256) *$($_.file)" } | Set-Content (Join-Path $staging "SHA256SUMS.txt") -Encoding ascii
[ordered]@{
    name = "KnightByte"
    version = $version
    commit = $commit
    working_tree_modified = $dirty
    rustc = $rustc
    built_utc = (Get-Date).ToUniversalTime().ToString("s") + "Z"
    node_pins = [ordered]@{ bench6 = 15528; bench12 = 456308; bench16 = 2815799; bench18 = 6892874 }
    artifacts = $entries
} | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $staging "manifest.json") -Encoding utf8
foreach ($file in "README.md", "LICENSE", "THIRD-PARTY-NOTICES.md") {
    Copy-Item (Join-Path $repo $file) $staging
}

$zip = Join-Path $out "$stem.zip"
Compress-Archive -Path (Join-Path $staging "*") -DestinationPath $zip
$zipHash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
"$zipHash *$stem.zip" | Set-Content (Join-Path $out "$stem.zip.sha256") -Encoding ascii

Write-Host ""
Write-Host "Release: $zip"
Write-Host "sha256 : $zipHash"
if ($dirty) { Write-Warning "the working tree has uncommitted changes; the manifest says so" }
