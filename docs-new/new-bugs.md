# KnightByte — New Bugs

No open bugs.

## Verified platforms

- **2026-11-17 — Windows 11 (build 26300), AMD Ryzen 7 260 (Zen 4, AVX-512/VNNI):**
  rustc 1.100.0 / cargo 1.100.0 (pinned by `rust-toolchain.toml`).
  `cargo build`, `cargo build --release`, `cargo test --all-targets` (with the
  FEN verifier set), `cargo clippy --all-targets -- -D warnings` and
  `cargo fmt --all -- --check` all pass. UCI smoke test returns `uciok`,
  `readyok`, `Nodes: 8902` for `perft 3` and `nnueeval 27`.
  No portability issues found.

## Template

When a new defect is found, append an entry with:

- **Date:**
- **Build / commit:**
- **Platform / CPU:**
- **Command or FEN:**
- **Expected:**
- **Actual:**
- **Reproduction steps:**
- **Status:** open / fixed / cannot reproduce
