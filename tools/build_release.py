#!/usr/bin/env python3
"""Build, verify and package the Windows x64 release artifacts.

Before anything is built the lever effect gate runs (every advertised lever
must change the search, ADR-0014). Then, for every variant of ADR-0011, the
script

  1. builds a profile-guided executable with tools/pgo_build.py (its own
     profile, the C runtime linked statically; the four bench node pins are
     verified),
  2. runs the file: `uci` must answer `uciok`, `bench 6` must report the pinned
     node count, and a run forced to the scalar kernels must report the same,
  3. checks the import table: only Windows system DLLs, and no Visual C++
     runtime (a plain Windows 10 machine does not have it),
  4. records the SHA-256 of the file.

It then writes SHA256SUMS.txt and manifest.json and packs the executables with
README.md, LICENSE and THIRD-PARTY-NOTICES.md into one zip.

Profiles describe one host: run this on the machine whose CPU supports the most
capable variant (the v4-vnni build is profiled by running it).

Usage:
    python tools/build_release.py
    python tools/build_release.py --variants v3 v4-vnni --out-dir dist
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import zipfile
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import pgo_build  # noqa: E402

REPO = Path(__file__).resolve().parent.parent

# Variant table: the executable's compile-time baseline (ADR-0011).
VARIANTS = {
    "v2": {"cpu": "x86-64-v2", "features": ""},
    "v3": {"cpu": "x86-64-v3", "features": ""},
    "v4-vnni": {"cpu": "x86-64-v4", "features": "+avx512vnni"},
}

ALLOWED_DLLS = {
    "kernel32.dll",
    "ntdll.dll",
    "bcryptprimitives.dll",
    "api-ms-win-core-synch-l1-2-0.dll",
}


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def bench6_nodes(exe, environment=None):
    result = subprocess.run(
        [str(exe)],
        input="bench 6\nquit\n",
        capture_output=True,
        text=True,
        check=True,
        env=dict(os.environ, **(environment or {})),
    )
    for line in result.stdout.splitlines():
        if line.startswith("Nodes:"):
            return int(line.split(":")[1].strip())
    raise RuntimeError("bench 6 printed no node total")


def imported_dlls(exe):
    output = subprocess.run(
        [sys.executable, str(REPO / "tools" / "pe_imports.py"), str(exe)],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return sorted({line.split(" ")[0].lower() for line in output.splitlines() if line and not line[0].isspace()})


def git(*args):
    return subprocess.run(["git", *args], cwd=REPO, capture_output=True, text=True).stdout.strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--out-dir", default="dist")
    parser.add_argument("--variants", nargs="+", default=["v2", "v3", "v4-vnni"], choices=sorted(VARIANTS))
    parser.add_argument("--skip-lever-gate", action="store_true", help="skip the lever effect gate")
    args = parser.parse_args()

    cargo_toml = (REPO / "Cargo.toml").read_text(encoding="utf8")
    version = re.search(r'^version\s*=\s*"([^"]+)"', cargo_toml, re.M).group(1)
    commit = git("rev-parse", "--short", "HEAD")
    dirty = bool(git("status", "--porcelain", "--untracked-files=no"))
    rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
    stem = f"knightbyte-{version}-windows-x64"

    out = (REPO / args.out_dir).resolve()
    staging = out / stem
    shutil.rmtree(out, ignore_errors=True)
    staging.mkdir(parents=True)

    if not args.skip_lever_gate:
        print("=== lever effect gate")
        subprocess.run(["cargo", "test", "--release", "--test", "lever_effect"], cwd=REPO, check=True)

    entries = []
    for variant in args.variants:
        spec = VARIANTS[variant]
        print(f"=== variant {variant} (target-cpu {spec['cpu']} {spec['features']})")
        exe = staging / f"knightbyte-{variant}.exe"
        pgo_build.build_pgo(
            work_dir=f"pgo-work-{variant}",
            output=str(exe),
            target_cpu=spec["cpu"],
            target_features=spec["features"],
        )

        # Behaviour checks on the packaged file itself.
        uci = subprocess.run([str(exe)], input="uci\nquit\n", capture_output=True, text=True, check=True).stdout
        if "uciok" not in uci:
            raise SystemExit(f"{variant} did not answer uciok")
        nodes = bench6_nodes(exe)
        if nodes != pgo_build.PINS[6]:
            raise SystemExit(f"{variant} bench 6 reported {nodes} nodes")
        # The slowest backend must search the identical tree (ADR-0001 gate).
        scalar = bench6_nodes(exe, {"KNIGHTBYTE_FORCE_ISA": "scalar"})
        if scalar != nodes:
            raise SystemExit(f"{variant} forced scalar reported {scalar} nodes")

        dlls = imported_dlls(exe)
        unexpected = [dll for dll in dlls if dll not in ALLOWED_DLLS]
        if unexpected:
            raise SystemExit(f"{variant} imports unexpected DLLs: {', '.join(unexpected)}")

        digest = sha256(exe)
        entries.append(
            {
                "variant": variant,
                "file": exe.name,
                "target_cpu": spec["cpu"],
                "target_features": spec["features"],
                "bytes": exe.stat().st_size,
                "sha256": digest,
                "bench6_nodes": nodes,
            }
        )
        print(f"  ok: {nodes} nodes, forced scalar identical, imports clean, sha256 {digest}")

    # Checksums, manifest, documentation, archive.
    (staging / "SHA256SUMS.txt").write_text(
        "".join(f"{entry['sha256']} *{entry['file']}\n" for entry in entries), encoding="ascii"
    )
    manifest = {
        "name": "KnightByte",
        "version": version,
        "commit": commit,
        "working_tree_modified": dirty,
        "rustc": rustc,
        "built_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "node_pins": {f"bench{depth}": nodes for depth, nodes in pgo_build.PINS.items()},
        "artifacts": entries,
    }
    (staging / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf8")
    for name in ("README.md", "LICENSE", "THIRD-PARTY-NOTICES.md"):
        shutil.copy2(REPO / name, staging / name)

    archive = out / f"{stem}.zip"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
        for path in sorted(staging.iterdir()):
            bundle.write(path, path.name)
    archive_hash = sha256(archive)
    (out / f"{stem}.zip.sha256").write_text(f"{archive_hash} *{archive.name}\n", encoding="ascii")

    print()
    print(f"Release: {archive}")
    print(f"sha256 : {archive_hash}")
    if dirty:
        print("WARNING: the working tree has uncommitted changes; the manifest says so", file=sys.stderr)


if __name__ == "__main__":
    sys.exit(main())
