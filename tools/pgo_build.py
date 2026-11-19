#!/usr/bin/env python3
"""Build a profile-guided (PGO) release binary of the engine.

1. builds an instrumented binary (-Cprofile-generate),
2. runs the deterministic bench workloads to collect profiles,
3. merges them with llvm-profdata,
4. rebuilds with -Cprofile-use.

The profile describes ONE host and ONE build configuration: generate it on the
machine (and with the target-cpu / feature set) the binary is built for, and
never reuse or check in the .profdata files. PGO must not change the search:
the four bench node pins are verified on the result and the build fails if any
of them moves.

Requires the llvm-tools component:  rustup component add llvm-tools-preview

Usage:
    python tools/pgo_build.py
    python tools/pgo_build.py --target-cpu x86-64-v3 --work-dir pgo-work-v3 \\
        --output target/pgo/knightbyte-v3.exe
    python tools/pgo_build.py --target-cpu x86-64-v4 --target-features +avx512vnni

`--target-cpu` builds a dedicated variant (for example x86-64-v3); without it
the executable is the portable build whose SIMD backend is chosen at run time
(ADR-0001, ADR-0011). The instrumented and the profile-use builds always use
the same flags.
"""

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# Node identity pins of the four-position bench (depths 6, 12, 16, 18).
PINS = {6: 15_528, 12: 456_308, 16: 2_815_799, 18: 6_892_874}


def host_triple_and_sysroot():
    sysroot = subprocess.check_output(["rustc", "--print", "sysroot"], text=True).strip()
    version = subprocess.check_output(["rustc", "-vV"], text=True)
    host = next(line.split()[1] for line in version.splitlines() if line.startswith("host:"))
    return host, Path(sysroot)


def rust_flags(host, target_cpu, target_features):
    """The code-generation flags shared by both builds of one variant."""
    flags = []
    # The C runtime is linked statically on Windows: a plain Windows 10 machine
    # has no VCRUNTIME140.dll unless the Visual C++ redistributable was
    # installed, and the engine must start without it. (RUSTFLAGS replaces the
    # flags of .cargo/config.toml, so the flag is repeated here.)
    if host.endswith("windows-msvc"):
        flags.append("-Ctarget-feature=+crt-static")
    if target_cpu:
        flags.append(f"-Ctarget-cpu={target_cpu}")
    if target_features:
        flags.append(f"-Ctarget-feature={target_features}")
    return " ".join(flags)


def cargo_build(target_dir, rustflags):
    environment = dict(os.environ, RUSTFLAGS=rustflags)
    subprocess.run(
        ["cargo", "build", "--release", "--target-dir", str(target_dir)],
        cwd=REPO,
        env=environment,
        check=True,
    )


def bench_nodes(binary, depth):
    """Runs `bench <depth>` through standard input and returns the node total."""
    result = subprocess.run(
        [str(binary)],
        input=f"bench {depth}\nquit\n",
        capture_output=True,
        text=True,
        check=True,
    )
    for line in result.stdout.splitlines():
        if line.startswith("Nodes:"):
            return int(line.split(":")[1].strip())
    raise RuntimeError(f"bench {depth} printed no node total")


def build_pgo(work_dir="pgo-work", output="target/pgo/knightbyte.exe", target_cpu="", target_features=""):
    """Builds the PGO binary, verifies the pins and copies it to `output`."""
    host, sysroot = host_triple_and_sysroot()
    suffix = ".exe" if os.name == "nt" else ""
    profdata = sysroot / "lib" / "rustlib" / host / "bin" / f"llvm-profdata{suffix}"
    if not profdata.exists():
        raise SystemExit("llvm-profdata not found; run: rustup component add llvm-tools-preview")

    flags = rust_flags(host, target_cpu, target_features)
    print(f"rustflags: {flags}")

    work = (REPO / work_dir).resolve()
    raw = work / "raw"
    merged = work / "merged.profdata"
    shutil.rmtree(work, ignore_errors=True)
    raw.mkdir(parents=True)

    print("== 1/4 instrumented build")
    cargo_build(work / "gen", f"{flags} -Cprofile-generate={raw}".strip())

    print("== 2/4 collecting profiles")
    instrumented = work / "gen" / "release" / f"knightbyte{suffix}"
    for depth in (12, 16):
        bench_nodes(instrumented, depth)

    print("== 3/4 merging")
    subprocess.run([str(profdata), "merge", "-o", str(merged), str(raw)], check=True)

    print("== 4/4 profile-use build")
    cargo_build(work / "use", f"{flags} -Cprofile-use={merged}".strip())

    binary = work / "use" / "release" / f"knightbyte{suffix}"
    print("== verifying the node pins")
    for depth, expected in sorted(PINS.items()):
        nodes = bench_nodes(binary, depth)
        if nodes != expected:
            raise SystemExit(
                f"PGO build moved the bench-{depth} pin: {nodes} (expected {expected}) - do not ship"
            )

    destination = Path(output)
    if not destination.is_absolute():
        destination = REPO / destination
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(binary, destination)
    print(f"PGO binary: {destination} (pins exact)")
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--work-dir", default="pgo-work", help="scratch directory (git-ignored)")
    parser.add_argument("--output", default="target/pgo/knightbyte.exe", help="where the binary is copied")
    parser.add_argument("--target-cpu", default="", help="for example x86-64-v3 (empty: portable build)")
    parser.add_argument("--target-features", default="", help="for example +avx512vnni")
    args = parser.parse_args()
    build_pgo(args.work_dir, args.output, args.target_cpu, args.target_features)


if __name__ == "__main__":
    sys.exit(main())
