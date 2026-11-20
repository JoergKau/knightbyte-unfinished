#!/usr/bin/env python3
"""Build, verify and package the Linux x64 release artifact.

The Linux release is one x86-64 binary. It is compiled for
`x86_64-unknown-linux-gnu` with `-Ctarget-cpu=x86-64-v3`, the AVX2 baseline
(Intel Haswell 2013 and newer, AMD Zen and newer), which lets the compiler use
AVX2 and BMI2 outside the dispatched kernels as well. The ARM platforms are
not supported and no other architecture is produced.

The default build is profile-guided (PGO), like the Windows and macOS
releases: the instrumented binary runs `bench 12` and `bench 16`, the profile
is merged and the final binary is rebuilt with it. The node pins are verified
afterwards, so a profile can never change the search. `--no-pgo` produces the
plain build. PGO needs `rustup component add llvm-tools-preview` once per
machine.

The script

  1. refuses to run anywhere else than an x86-64 Linux host,
  2. runs the lever effect gate (every advertised lever must change the
     search, ADR-0014) unless it is skipped,
  3. builds the release binary (PGO by default) with the AVX2 baseline,
  4. verifies the file itself: `uci` must answer `uciok`, the four bench node
     pins must be exact, and runs forced to the scalar kernels and to the
     shipped SIMD backend must report the same totals (the kernel-identity
     gate),
  5. checks the ELF architecture (x86-64 only) and records the SHA-256,
  6. writes SHA256SUMS.txt and manifest.json and packs the binary with
     README.md, LICENSE and THIRD-PARTY-NOTICES.md into one tar.gz.

Older processors: the AVX2 build refuses to start on a CPU without AVX2, BMI2
and FMA and names the missing feature instead of crashing. `--target-cpu
x86-64-v2` produces the portable fallback for such machines.

Usage:
    python3 tools/build_release_linux.py
    python3 tools/build_release_linux.py --no-pgo
    python3 tools/build_release_linux.py --target-cpu x86-64-v2 --out-dir dist
"""

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import struct
import subprocess
import sys
import tarfile
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import pgo_build  # noqa: E402

REPO = Path(__file__).resolve().parent.parent

# The shipped baseline: the x86-64-v3 feature set is AVX2, BMI2, FMA and the
# earlier levels. Every CPU from Haswell (2013) and Zen (2017) provides it.
TARGET = "x86_64-unknown-linux-gnu"
DEFAULT_TARGET_CPU = "x86-64-v3"

# ELF identifiers written by the linker: 64-bit, little-endian, EM_X86_64.
ELF_CLASS_64 = 2
ELF_DATA_LSB = 1
ELF_MACHINE_X86_64 = 62


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


def elf_architecture(binary):
    """Returns the ELF class, data encoding and machine of the file.

    Parsed directly instead of shelling out to `file`, so the check works on a
    minimal CI image without binutils. The first 20 bytes hold the identifier
    and the machine field (offset 18, little-endian on x86-64).
    """
    with open(binary, "rb") as handle:
        header = handle.read(20)
    if len(header) < 20 or header[:4] != b"\x7fELF":
        raise SystemExit(f"{binary} is not an ELF file")
    return header[4], header[5], struct.unpack_from("<H", header, 18)[0]


def git(*args):
    return subprocess.run(["git", *args], cwd=REPO, capture_output=True, text=True).stdout.strip()


def require_linux_x64():
    """The release must be built and verified on an x86-64 Linux host."""
    if sys.platform != "linux":
        raise SystemExit("the Linux release must be built on Linux")
    if platform.machine() not in ("x86_64", "amd64"):
        raise SystemExit(
            "the Linux release must be built on an x86-64 host; ARM is not supported"
        )


def host_has_avx2():
    """Returns whether the build host advertises AVX2 in `/proc/cpuinfo`."""
    try:
        with open("/proc/cpuinfo", encoding="utf8") as handle:
            for line in handle:
                if line.startswith("flags"):
                    return "avx2" in line.split(":", 1)[1].split()
    except OSError:
        pass
    return False


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
        "--target-cpu",
        default=DEFAULT_TARGET_CPU,
        help=f"LLVM CPU baseline (default {DEFAULT_TARGET_CPU}; x86-64-v2 for older CPUs)",
    )
    parser.add_argument(
        "--target-dir",
        default="target/linux-release",
        help="cargo target directory for the plain build (kept out of the dev build)",
    )
    args = parser.parse_args()

    require_linux_x64()
    if args.target_cpu == DEFAULT_TARGET_CPU and not host_has_avx2():
        raise SystemExit(
            "the AVX2 release must be built on a host with AVX2; "
            "use --target-cpu x86-64-v2 for an older machine"
        )

    cargo_toml = (REPO / "Cargo.toml").read_text(encoding="utf8")
    version = re.search(r'^version\s*=\s*"([^"]+)"', cargo_toml, re.M).group(1)
    commit = git("rev-parse", "--short", "HEAD")
    dirty = bool(git("status", "--porcelain", "--untracked-files=no"))
    rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
    stem = f"knightbyte-{version}-linux-x64"
    flags = f"-Ctarget-cpu={args.target_cpu}"

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
        print(f"=== PGO build ({TARGET}, target-cpu {args.target_cpu})")
        pgo_build.build_pgo(
            work_dir="target/pgo-work/linux",
            output=str(binary),
            target_cpu=args.target_cpu,
            keep_work=False,
        )
    else:
        print(f"=== plain build ({TARGET}, target-cpu {args.target_cpu})")
        environment = dict(os.environ, RUSTFLAGS=flags)
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

    # The scalar reference and the shipped SIMD backend must search the
    # identical tree. The AVX2 build is verified on the AVX2 backend; a v2
    # fallback built on an older host is verified on the SSE4.1 backend.
    for backend in ("scalar", "avx2" if host_has_avx2() else "v2"):
        nodes = bench_nodes(binary, 12, {"KNIGHTBYTE_FORCE_ISA": backend})
        if nodes != pgo_build.PINS[12]:
            raise SystemExit(f"forced {backend} reported {nodes} nodes")

    elf_class, elf_data, elf_machine = elf_architecture(binary)
    if (elf_class, elf_data, elf_machine) != (
        ELF_CLASS_64,
        ELF_DATA_LSB,
        ELF_MACHINE_X86_64,
    ):
        raise SystemExit(
            "the binary must be a 64-bit little-endian x86-64 ELF, "
            f"found class {elf_class}, data {elf_data}, machine {elf_machine}"
        )

    digest = sha256(binary)
    print(f"  ok: pins exact, scalar and SIMD backends identical, x86-64 ELF, sha256 {digest}")

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
        "target_cpu": args.target_cpu,
        "pgo": pgo,
        "architectures": ["x86_64"],
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
