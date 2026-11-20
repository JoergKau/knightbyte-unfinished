#!/usr/bin/env python3
"""Build, verify and package the Linux x64 release artifact.

The Linux release is one x86-64 binary. It is compiled for
`x86_64-unknown-linux-gnu` with `-Ctarget-cpu=x86-64-v3`, the AVX2 baseline
(Intel Haswell 2013 and newer, AMD Zen and newer), which lets the compiler use
AVX2 and BMI2 outside the dispatched kernels as well. The ARM platforms are
not supported and no other architecture is produced.

PGO is the release build, not a choice: the default is profile-guided (as on
Windows and macOS), and `--no-pgo` exists only to measure what the profile
buys on this host. The instrumented binary runs `bench 12` and `bench 16`, the
profile is merged and the final binary is rebuilt with it. The node pins are
verified afterwards, so a profile can never change the search. PGO needs
`rustup component add llvm-tools-preview` once per machine; that is checked
before anything is built or replaced.

The script

  1. refuses to run anywhere else than an x86-64 Linux host,
  2. checks the PGO toolchain when the profile build is requested,
  3. runs the lever effect gate (every advertised lever must change the
     search, ADR-0014) unless it is skipped,
  4. builds the release binary (PGO by default) with the AVX2 baseline,
  5. verifies the file itself: `uci` must answer `uciok`, the four bench node
     pins must be exact, and runs forced to the scalar kernels and to the
     shipped SIMD backend must report the same totals (the kernel-identity
     gate),
  6. checks the ELF architecture (x86-64 only) and records the SHA-256,
  7. writes SHA256SUMS.txt and manifest.json and packs the binary with
     README.md, LICENSE and THIRD-PARTY-NOTICES.md into one tar.gz.

Output: the tar.gz and its .sha256 land in `--out-dir` (default `dist/`) and a
successful run leaves nothing else behind. The whole build - the lever gate,
the plain/PGO builds and the unpacked package - happens in one scratch tree
under `target/` that is deleted after a successful run (a failed run keeps it
for inspection), so `target/` is not filled with a release cache and the
developer's own `cargo build` cache is never touched.

Pass `--keep-cache` to keep that tree for a fast re-run and `--keep-staging` to
keep the unpacked package next to the archive. A previous artifact is replaced
only after the new build has passed every check, so a failed run cannot destroy
a good release. `--variant-suffix` gives a measurement build its own name (for
example `--variant-suffix plain` produces `...-linux-x64-plain.tar.gz`) so it
cannot collide with the release artifact.

Older processors: the AVX2 build refuses to start on a CPU without AVX2, BMI2
and FMA and names the missing feature instead of crashing. `--target-cpu
x86-64-v2` produces the portable fallback for such machines.

Usage:
    python3 tools/build_release_linux.py
    python3 tools/build_release_linux.py --no-pgo
    python3 tools/build_release_linux.py --target-cpu x86-64-v2 --out-dir dist
    python3 tools/build_release_linux.py --no-pgo --variant-suffix plain
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


def require_profdata():
    """Fails before any build or file replacement when the PGO merger is missing."""
    host, sysroot = pgo_build.host_triple_and_sysroot()
    suffix = ".exe" if os.name == "nt" else ""
    tool = sysroot / "lib" / "rustlib" / host / "bin" / f"llvm-profdata{suffix}"
    if not tool.exists():
        raise SystemExit("llvm-profdata not found; run: rustup component add llvm-tools-preview")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--out-dir", default="dist", help="where the archive and its checksum are written")
    parser.add_argument("--skip-lever-gate", action="store_true", help="skip the lever effect gate")
    parser.add_argument(
        "--no-pgo",
        action="store_true",
        help="measurement build without a profile; the default (PGO) is the release build",
    )
    parser.add_argument(
        "--variant-suffix",
        default="",
        help="suffix for a non-release artifact name, e.g. pgo or plain, so two variants can coexist",
    )
    parser.add_argument(
        "--keep-staging",
        action="store_true",
        help="also keep the unpacked package folder in --out-dir",
    )
    parser.add_argument(
        "--keep-cache",
        action="store_true",
        help="keep the build cache under target/ (default: delete it, so only the release files remain)",
    )
    parser.add_argument(
        "--target-cpu",
        default=DEFAULT_TARGET_CPU,
        help=f"LLVM CPU baseline (default {DEFAULT_TARGET_CPU}; x86-64-v2 for older CPUs)",
    )
    parser.add_argument(
        "--target-dir",
        default="target",
        help="cargo target directory; the scratch build tree is created underneath it",
    )
    args = parser.parse_args()

    require_linux_x64()
    if args.target_cpu == DEFAULT_TARGET_CPU and not host_has_avx2():
        raise SystemExit(
            "the AVX2 release must be built on a host with AVX2; "
            "use --target-cpu x86-64-v2 for an older machine"
        )

    # The profile build is the release build; `--no-pgo` is the measurement
    # variant. The toolchain check runs first so a missing component cannot
    # waste the lever gate, and cannot leave an older artifact half-replaced.
    pgo = not args.no_pgo
    if pgo:
        require_profdata()

    cargo_toml = (REPO / "Cargo.toml").read_text(encoding="utf8")
    version = re.search(r'^version\s*=\s*"([^"]+)"', cargo_toml, re.M).group(1)
    commit = git("rev-parse", "--short", "HEAD")
    dirty = bool(git("status", "--porcelain", "--untracked-files=no"))
    rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
    stem = f"knightbyte-{version}-linux-x64"
    if args.variant_suffix:
        stem = f"{stem}-{args.variant_suffix}"
    flags = f"-Ctarget-cpu={args.target_cpu}"

    out = (REPO / args.out_dir).resolve()
    target_dir = (REPO / args.target_dir).resolve()
    # Everything the build needs lives in one scratch tree under target/. It is
    # deleted after a successful run (unless --keep-cache), so a release leaves
    # only the two published files and never mixes into the developer's own
    # `cargo build` cache.
    scratch = target_dir / "release-build"
    shutil.rmtree(scratch, ignore_errors=True)
    staging = scratch / "packaging" / stem
    staging.mkdir(parents=True)

    if not args.skip_lever_gate:
        print("=== lever effect gate", flush=True)
        # The gate is a release-profile test build. Its own target directory
        # keeps about 90 MB of build scripts out of the dev cache and lets the
        # whole thing disappear with the rest of the scratch tree.
        subprocess.run(
            ["cargo", "test", "--release", "--test", "lever_effect"],
            cwd=REPO,
            env=dict(os.environ, CARGO_TARGET_DIR=str(scratch / "gate")),
            check=True,
        )

    # The profile is generated on this host and never checked in; the pins are
    # verified on the finished binary by pgo_build itself and again below.
    binary = staging / "knightbyte"
    if pgo:
        print(f"=== PGO build ({TARGET}, target-cpu {args.target_cpu})", flush=True)
        pgo_build.build_pgo(
            work_dir=str(scratch / "pgo"),
            output=str(binary),
            target_cpu=args.target_cpu,
            keep_work=False,
        )
    else:
        print(f"=== plain build, no profile ({TARGET}, target-cpu {args.target_cpu})", flush=True)
        environment = dict(os.environ, RUSTFLAGS=flags)
        subprocess.run(
            [
                "cargo",
                "build",
                "--release",
                "--target",
                TARGET,
                "--target-dir",
                str(scratch / "plain"),
            ],
            cwd=REPO,
            env=environment,
            check=True,
        )
        built = scratch / "plain" / TARGET / "release" / "knightbyte"
        if not built.exists():
            raise SystemExit(f"the build produced no binary at {built}")
        shutil.copy2(built, binary)

    # Behaviour checks on the packaged file itself.
    print("=== verify", flush=True)
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

    # Every check has passed, so only now is the previous artifact replaced.
    # The archive is packed in the scratch tree first and moved into `out`
    # afterwards, so a failed build can never destroy a good release.
    archive_staged = scratch / "packaging" / f"{stem}.tar.gz"
    with tarfile.open(archive_staged, "w:gz") as bundle:
        for path in sorted(staging.iterdir()):
            bundle.add(path, arcname=f"{stem}/{path.name}")
    archive_hash = sha256(archive_staged)

    out.mkdir(parents=True, exist_ok=True)
    archive = out / f"{stem}.tar.gz"
    (out / f"{stem}.tar.gz.sha256").unlink(missing_ok=True)
    shutil.rmtree(out / stem, ignore_errors=True)
    if args.keep_staging:
        shutil.copytree(staging, out / stem)
    shutil.move(str(archive_staged), archive)
    (out / f"{stem}.tar.gz.sha256").write_text(f"{archive_hash} *{archive.name}\n", encoding="ascii")

    # A release run leaves the published files and nothing else. The build
    # cache is disposable: --keep-cache keeps it for a fast re-run, otherwise
    # it is deleted, and an empty target/ created by this run is removed too.
    if args.keep_cache:
        print(f"cache : {scratch} (--keep-cache)")
    else:
        shutil.rmtree(scratch, ignore_errors=True)
        try:
            target_dir.rmdir()
        except OSError:
            pass

    print()
    print(f"wrote  : {archive}")
    print(f"wrote  : {archive}.sha256")
    if args.keep_staging:
        print(f"wrote  : {out / stem}/")
    if dirty:
        print("WARNING: the working tree has uncommitted changes; the manifest says so", file=sys.stderr)


if __name__ == "__main__":
    sys.exit(main())
