# Rust how-to: KnightByte

KnightByte is a headless **UCI chess engine** written in Rust. It speaks UCI on
stdin/stdout and has no graphical interface. The toolchain is pinned by
`rust-toolchain.toml` (Rust 1.100.0, with `rustfmt` and `clippy`), so any
`cargo`/`rustup` command run inside this directory automatically uses that
toolchain.

All commands below are run from the repository root (`C:\dev\knightbyte-unfinished`).

---

## 1. Build / compile

| Command | What it does |
|---|---|
| `cargo build` | Debug build → `target\debug\knightbyte.exe` (fast compile, slow engine) |
| `cargo build --release` | Optimized build → `target\release\knightbyte.exe` (**use this for actual play**) |
| `cargo check` | Type-checks everything without producing a binary — fastest feedback loop |
| `cargo check --all-targets` | Also type-checks tests and examples |
| `cargo clean` | Deletes the whole `target/` directory (full rebuild next time) |
| `cargo build --release -v` | Verbose build output, useful for debugging build issues |

The release profile (`Cargo.toml`) uses `opt-level = 3`, fat LTO,
`codegen-units = 1`, `panic = "abort"` and symbol stripping, so release builds
take noticeably longer than debug builds — that is expected.

The NNUE network `net/jokausch.nnue` is **embedded into the binary at compile
time**. There is no external network file to ship or configure at runtime.

## 2. Run the engine

```powershell
# Run the release build directly
.\target\release\knightbyte.exe

# Or let cargo build + run in one step
cargo run
cargo run --release
```

Then type UCI commands by hand, e.g.:

```text
uci
isready
ucinewgame
position startpos
go depth 6
quit
```

Supported UCI commands: `uci`, `isready`, `ucinewgame`, `setoption`,
`position` (`startpos` or a six-field FEN), `go` (`depth` / `nodes` /
`movetime` / clock limiters), `bench`, `perft`, `nnueeval`, `quit`.

## 3. Engine diagnostics (from the UCI prompt)

```text
perft 5            ; legal-move node counts for the current position
nnueeval           ; NNUE static evaluation of the current position
bench 12           ; deterministic benchmark; the parity gate is "bench 12"
go depth 10        ; fixed-depth search
go movetime 1000   ; search for 1 second
```

## 4. Test

| Command | What it does |
|---|---|
| `cargo test` | Runs all unit + integration tests |
| `cargo test --release` | Same, but with optimizations (perft tests run much faster) |
| `cargo test perft` | Only tests with `perft` in their name/path |
| `cargo test --test uci_smoke` | Runs one integration test file (`uci_smoke`, `search`, `perft`, `nnue_parity`, `move_ordering`, `fen_verified`) |
| `cargo test -- --nocapture` | Shows `println!` output from tests |
| `cargo test -- --exact golden_perft` | Runs a single test by exact name |

Most tests require an external FEN verifier. Set it before running the suite:

```powershell
# PowerShell (this session)
$env:KNIGHTBYTE_FEN_VERIFIER = "C:\path\to\uci-verifier.exe"
cargo test
```

The verifier is a local test tool and is not distributed with KnightByte.

## 5. Lint and format (CI runs these — check before committing)

```powershell
cargo fmt --all                    # format everything
cargo fmt --all -- --check        # CI-style check, no changes written
cargo clippy --all-targets -- -D warnings   # CI-style lint, fail on warnings
```

Note: `[lints.rust] warnings = "deny"` is set in `Cargo.toml`, so ordinary
`cargo build` also fails on warnings.

## 6. Dependencies

```powershell
cargo update          # bump dependencies within Cargo.lock semver ranges
cargo tree            # show the dependency tree
cargo doc --open      # build and open rustdoc for the crate
```

There are no runtime dependencies; `sha2` is test-only (network checksum).

## 7. Toolchain management

`rust-toolchain.toml` pins the version — `rustup` installs/uses it
automatically. Useful commands:

```powershell
rustup show                # which toolchain this project resolves to
rustup toolchain list      # installed toolchains
rustup update              # update toolchains
rustfmt --version          # confirm the pinned formatter is active
```

## 8. Useful git workflow reminders (from README)

* Never copy or commit `target/` — it contains host-specific build artifacts.
* Pull before a work session; commit after each completed step.

---

## 9. Playing against KnightByte with a chess GUI

The engine has no GUI of its own — a UCI-compliant chess GUI drives it over
stdin/stdout. You do **not** point the GUI at a source file or the network:
you point it at the **engine executable**.

### The path to give your GUI

| Build | Path |
|---|---|
| Release (recommended for play) | `C:\dev\knightbyte-unfinished\target\release\knightbyte.exe` |
| Debug (slow, for development) | `C:\dev\knightbyte-unfinished\target\debug\knightbyte.exe` |

### Steps

1. Build the release binary first: `cargo build --release`.
2. In your GUI's engine-management dialog ("Add engine" / "Install engine"),
   select the `.exe` path above and declare it as a **UCI engine** (WinBoard/
   XBoard protocol is not supported — KnightByte only speaks UCI).
3. Nothing else needs configuring: the NNUE network is embedded in the binary,
   so there is no `EvalFile`/network file to select in the GUI. (The `EvalFile`
   option is accepted but inert.)

Common GUIs that work this way: Arena, Banksia GUI, Cute Chess (cutechess-cli),
En Croissant, Fritz/ChessBase, SCID vs. PC.

## One practical note

`KNIGHTBYTE_FEN_VERIFIER` is a per-session PowerShell variable, not persisted. Every new terminal needs:

```powershell
$env:KNIGHTBYTE_FEN_VERIFIER = (Get-ChildItem "C:\Program Files\ChessEngines" -Recurse -Filter "stockfish*.exe" | Select-Object -First 1).FullName
```

(You could set it permanently with `[Environment]::SetEnvironmentVariable(...,'User')` if retyping it gets annoying.)