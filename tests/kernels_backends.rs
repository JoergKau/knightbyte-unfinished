//! ADR-0001 gate: node-count identity across the SIMD backends.
//!
//! `KNIGHTBYTE_FORCE_ISA` pins each backend in a child process; every backend
//! must report the same `bench 6` node total, because the kernels are
//! integer-exact by construction. The forced backends that the host cannot
//! run are skipped (the universal executable contains them all, but a host
//! without AVX-512 cannot execute the 512-bit kernels).

use std::io::Write;
use std::process::{Command, Stdio};

/// The exact bench 6 pin every backend must reproduce.
const BENCH6_NODES: &str = "15528";

#[test]
fn every_forced_backend_reports_the_identical_bench_total() {
    // The 512-bit rung needs the host feature at RUNTIME: the universal
    // executable contains all backends regardless of the build features.
    let avx512_host = cfg!(target_arch = "x86_64")
        && std::arch::is_x86_feature_detected!("avx512f")
        && std::arch::is_x86_feature_detected!("avx512bw")
        && std::arch::is_x86_feature_detected!("avx512vl");
    let backends: [(&str, bool); 4] = [
        ("scalar", true),
        ("v2", cfg!(target_arch = "x86_64")),
        ("v3", cfg!(target_arch = "x86_64")),
        ("v4-vnni", avx512_host),
    ];

    for (backend, host_runs_it) in backends {
        if !host_runs_it {
            continue;
        }
        let mut child = Command::new(env!("CARGO_BIN_EXE_knightbyte"))
            .env("KNIGHTBYTE_FORCE_ISA", backend)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn knightbyte");
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(b"bench 6\nquit\n")
            .expect("write bench commands");
        let output = child.wait_with_output().expect("wait for knightbyte");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let nodes_line = stdout
            .lines()
            .find(|line| line.starts_with("Nodes: "))
            .expect("bench total");
        assert_eq!(
            nodes_line,
            &format!("Nodes: {BENCH6_NODES}"),
            "backend {backend} diverged"
        );
    }
}
