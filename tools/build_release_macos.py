#!/usr/bin/env python3
"""Build, verify and package the macOS Apple Silicon release artifact.

The macOS release is deliberately simpler than the Windows one: there is one
Apple Silicon build and no per-chip variants. The binary is compiled for the
`aarch64-apple-darwin` target with `-Ctarget-cpu=apple-m1`, which is the
baseline of every Apple M processor (M1 through M5) and enables NEON. Intel
Macs are not supported and no x86_64 slice is produced.

The default build is profile-guided (PGO), like the Windows release: the
instrumented binary runs `bench 12` and `bench 16`, the profile is merged and
the final binary is rebuilt with it. The node pins are verified afterwards, so
a profile can never change the search. `--no-pgo` produces the plain build.
PGO needs `rustup component add llvm-tools-preview` once per machine.

The script

  1. refuses to run anywhere else than an arm64 macOS host,
  2. runs the lever effect gate (every advertised lever must change the
     search, ADR-0014) unless it is skipped,
  3. builds the release binary (PGO by default) with `-Ctarget-cpu=apple-m1`,
  4. verifies the file itself: `uci` must answer `uciok`, the four bench node
     pins must be exact, and a run forced to the scalar kernels and one forced
     to NEON must report the same totals (the kernel-identity gate),
  5. checks the Mach-O architecture (arm64 only) and records the SHA-256,
  6. writes SHA256SUMS.txt and manifest.json and packs the binary with
     README.md, LICENSE and THIRD-PARTY-NOTICES.md into one tar.gz.

Usage:
    python3 tools/build_release_macos.py
    python3 tools/build_release_macos.py --no-pgo
    python3 tools/build_release_macos.py --out-dir dist --skip-lever-gate
"""

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tarfile
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import pgo_build  # noqa: E402

REPO = Path(__file__).resolve().parent.parent

# The Apple Silicon baseline: NEON, DotProd, LSE and RCPC are present on every
# M1-M5, so one binary covers the whole supported range. Intel Macs are not
# supported.
TARGET = "aarch64-apple-darwin"
TARGET_CPU = "apple-m1"
RUSTFLAGS = f"-Ctarget-cpu={TARGET_CPU}"


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def bench_nodes(binary, depth, environment=None):
    """Runs `bench <depth>` through standard input and returns the node total."""
    result = subprocess.run(
        [str(binary)],
        input=f"bench {depth}\nquit\n",
        capture_output=True,
        text=True,
        check=True,
        env=dict(os.environ, **(environment or {})),
    )
    for line in result.stdout.splitlines():
        if line.startswith("Nodes:"):
            return int(line.split(":")[1].strip())
    raise RuntimeError(f"bench {depth} printed no node total")


def architecture(binary):
    """Returns the Mach-O architectures as reported by `lipo`."""
    output = subprocess.run(
        ["lipo", "-archs", str(binary)], capture_output=True, text=True, check=True
    ).stdout
    return output.split()


def git(*args):
    return subprocess.run(["git", *args], cwd=REPO, capture_output=True, text=True).stdout.strip()


def require_apple_silicon():
    """The release must be built and verified on an arm64 macOS host."""
    if sys.platform != "darwin":
        raise SystemExit("the macOS release must be built on macOS")
    if platform.machine() != "arm64":
        raise SystemExit(
            "the macOS release must be built on an arm64 host; Intel Macs are not supported"
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--out-dir", default="dist")
    parser.add_argument("--skip-lever-gate", action="store_true", help="skip the lever effect gate")
    parser.add_argument(
        "--no-pgo",
        action="store_true",
        help="build without a profile (the default is profile-guided)",
    )
    parser.add_argument(
        "--target-dir",
        default="target/macos-release",
        help="cargo target directory for the plain build (kept out of the dev build)",
    )
    args = parser.parse_args()

    require_apple_silicon()

    cargo_toml = (REPO / "Cargo.toml").read_text(encoding="utf8")
    version = re.search(r'^version\s*=\s*"([^"]+)"', cargo_toml, re.M).group(1)
    commit = git("rev-parse", "--short", "HEAD")
    dirty = bool(git("status", "--porcelain", "--untracked-files=no"))
    rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
    stem = f"knightbyte-{version}-macos-arm64"

    out = (REPO / args.out_dir).resolve()
    staging = out / stem
    shutil.rmtree(out, ignore_errors=True)
    staging.mkdir(parents=True)

    if not args.skip_lever_gate:
        print("=== lever effect gate")
        subprocess.run(["cargo", "test", "--release", "--test", "lever_effect"], cwd=REPO, check=True)

    # The profile is generated on this host and never checked in; the pins are
    # verified on the finished binary by pgo_build itself and again below.
    pgo = not args.no_pgo
    binary = staging / "knightbyte"
    if pgo:
        print(f"=== PGO build ({TARGET}, target-cpu {TARGET_CPU})")
        pgo_build.build_pgo(
            work_dir="target/pgo-work/macos",
            output=str(binary),
            target_cpu=TARGET_CPU,
            keep_work=False,
        )
    else:
        print(f"=== plain build ({TARGET}, target-cpu {TARGET_CPU})")
        environment = dict(os.environ, RUSTFLAGS=RUSTFLAGS)
        subprocess.run(
            [
                "cargo",
                "build",
                "--release",
                "--target",
                TARGET,
                "--target-dir",
                str(REPO / args.target_dir),
            ],
            cwd=REPO,
            env=environment,
            check=True,
        )
        built = REPO / args.target_dir / TARGET / "release" / "knightbyte"
        if not built.exists():
            raise SystemExit(f"the build produced no binary at {built}")
        shutil.copy2(built, binary)

    # Behaviour checks on the packaged file itself.
    print("=== verify")
    uci = subprocess.run([str(binary)], input="uci\nquit\n", capture_output=True, text=True, check=True).stdout
    if "uciok" not in uci:
        raise SystemExit("the binary did not answer uciok")

    pins = {}
    for depth, expected in sorted(pgo_build.PINS.items()):
        nodes = bench_nodes(binary, depth)
        if nodes != expected:
            raise SystemExit(f"bench {depth} reported {nodes} nodes (expected {expected})")
        pins[f"bench{depth}"] = nodes

    # The scalar reference and the NEON backend must search the identical tree.
    for backend in ("scalar", "neon"):
        nodes = bench_nodes(binary, 12, {"KNIGHTBYTE_FORCE_ISA": backend})
        if nodes != pgo_build.PINS[12]:
            raise SystemExit(f"forced {backend} reported {nodes} nodes")

    arches = architecture(binary)
    if arches != ["arm64"]:
        raise SystemExit(f"the binary must be arm64 only, found: {', '.join(arches)}")

    digest = sha256(binary)
    print(f"  ok: pins exact, scalar and NEON identical, arm64 only, sha256 {digest}")

    # Checksums, manifest, documentation, archive.
    (staging / "SHA256SUMS.txt").write_text(
        f"{digest} *knightbyte\n", encoding="ascii"
    )
    manifest = {
        "name": "KnightByte",
        "version": version,
        "commit": commit,
        "working_tree_modified": dirty,
        "rustc": rustc,
        "built_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "target": TARGET,
        "target_cpu": TARGET_CPU,
        "pgo": pgo,
        "architectures": arches,
        "node_pins": pins,
        "artifacts": [
            {
                "file": binary.name,
                "bytes": binary.stat().st_size,
                "sha256": digest,
                "bench6_nodes": pins["bench6"],
            }
        ],
    }
    (staging / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf8")
    for name in ("README.md", "LICENSE", "THIRD-PARTY-NOTICES.md"):
        shutil.copy2(REPO / name, staging / name)

    archive = out / f"{stem}.tar.gz"
    with tarfile.open(archive, "w:gz") as bundle:
        for path in sorted(staging.iterdir()):
            bundle.add(path, arcname=f"{stem}/{path.name}")
    archive_hash = sha256(archive)
    (out / f"{stem}.tar.gz.sha256").write_text(f"{archive_hash} *{archive.name}\n", encoding="ascii")

    print()
    print(f"Release: {archive}")
    print(f"sha256 : {archive_hash}")
    if dirty:
        print("WARNING: the working tree has uncommitted changes; the manifest says so", file=sys.stderr)


if __name__ == "__main__":
    sys.exit(main())
