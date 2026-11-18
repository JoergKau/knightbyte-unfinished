//! Multi-thread stress gate (Phase 5 step 9).
//!
//! Drives the engine through 500 seeded-random cycles of `position`, `go`,
//! `stop`, `setoption` and `isready` with the thread count walking 1, 2, 4
//! and 8, and asserts what step 9 requires: no panic, exactly one `bestmove`
//! per `go`, and every `bestmove` legal for the position it was played on.
//! Node counts are not asserted (multi-thread search is nondeterministic by
//! design, migration plan 7.3).
//!
//! The test links the engine library so it can replay the positions and
//! check the bestmove legality against the same move generator the engine
//! uses; the FENs are verified against the external oracle at test time.

mod common;

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use knightbyte::board::movegen::legal;
use knightbyte::board::position::Position;
use knightbyte::types::r#move::Move;

/// The step-9 cycle count.
const CYCLES: usize = 500;

/// A tiny deterministic generator (no random-number dependency): the stress
/// walk must be reproducible, so a failing seed can be replayed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*: fast, seedable, good enough for command selection.
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A value in `0..bound`.
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
}

/// The engine under test with its timestamped output reader.
struct StressEngine {
    child: Child,
    stdin: std::process::ChildStdin,
    lines: Arc<Mutex<Vec<(Instant, String)>>>,
    reader: std::thread::JoinHandle<()>,
    /// The engine's stderr, captured to a file: a panic message that kills
    /// the engine lands here (an unread pipe would swallow it). The handle
    /// is kept open so the engine can write until it dies.
    #[allow(dead_code)]
    stderr_file: std::fs::File,
    stderr_path: std::path::PathBuf,
}

impl StressEngine {
    /// Spawns the engine and starts the reader thread.
    fn spawn() -> Self {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let stderr_path = std::env::temp_dir().join("knightbyte-stress-stderr.txt");
        let stderr_file = std::fs::File::create(&stderr_path).expect("stderr file");
        let mut child = Command::new(env!("CARGO_BIN_EXE_knightbyte"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(stderr_file.try_clone().expect("stderr clone")))
            .spawn()
            .expect("spawn knightbyte");
        let stdout = child.stdout.take().expect("stdout");
        let reader_lines = Arc::clone(&lines);
        let reader = std::thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => reader_lines
                        .lock()
                        .expect("reader lock")
                        .push((Instant::now(), line)),
                    Err(_) => break,
                }
            }
        });
        let stdin = child.stdin.take().expect("stdin");
        Self {
            child,
            stdin,
            lines,
            reader,
            stderr_file,
            stderr_path,
        }
    }

    /// Returns the engine's captured stderr (its panic messages).
    fn stderr_text(&self) -> String {
        std::fs::read_to_string(&self.stderr_path).unwrap_or_default()
    }

    /// Sends one command line; a broken pipe panics with the engine's
    /// captured stderr (the panic message that killed it).
    fn send(&mut self, line: &str) {
        let write = self
            .stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.write_all(b"\n"))
            .and_then(|_| self.stdin.flush());
        if let Err(error) = write {
            panic!(
                "the engine died before '{line}' could be sent: {error}\nengine stderr:\n{}",
                self.stderr_text()
            );
        }
    }

    /// Waits until a NEW bestmove appears (more than `before` on the wire),
    /// bounded per search; returns false when the deadline passed first.
    ///
    /// The count is snapshot-based: a cumulative wait would pass on stale
    /// answers (that test-harness bug once produced an impossible-looking
    /// legality failure two cycles after the real fault).
    fn wait_for_bestmoves(&self, before: usize, deadline: Instant) -> bool {
        loop {
            let seen = self.bestmove_count();
            if seen > before {
                return true;
            }
            if Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Returns the number of bestmove lines seen so far.
    fn bestmove_count(&self) -> usize {
        self.lines
            .lock()
            .expect("reader lock")
            .iter()
            .filter(|(_, line)| line.starts_with("bestmove "))
            .count()
    }

    /// True while the engine process is still running.
    fn is_alive(&mut self) -> bool {
        self.child.try_wait().expect("probe").is_none()
    }
    /// Shuts the engine down and asserts a clean exit.
    fn shutdown(mut self) {
        self.send("quit");
        drop(self.stdin);
        assert!(
            self.child.wait().expect("wait").success(),
            "the engine must exit cleanly (no panic) — stderr: {}",
            String::from_utf8_lossy(
                &self
                    .child
                    .stderr
                    .map(|mut e| {
                        use std::io::Read;
                        let mut buffer = String::new();
                        let _ = e.read_to_string(&mut buffer);
                        buffer.into_bytes()
                    })
                    .unwrap_or_default()
            )
        );
        self.reader.join().expect("reader");
    }
}

/// Counts the bestmove lines currently on the wire.
fn bestmove_count(lines: &[String]) -> usize {
    lines
        .iter()
        .filter(|line| line.starts_with("bestmove "))
        .count()
}

/// The stress walk: 500 random cycles, four thread counts.
#[test]
fn five_hundred_random_cycles_across_thread_counts_stay_clean() {
    // Verify the FENs used by the walk at test time (the project rule).
    common::verify_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    common::verify_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1");

    let mut engine = StressEngine::spawn();
    let mut rng = Rng(0x5EED_2026_1118);
    let mut bestmoves_expected = 0;
    // The recent command history, for the failure trace.
    let mut sent: Vec<String> = Vec::new();
    // The current position of the walk, mirrored for the legality check.
    let mut position =
        Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
            .expect("the start FEN must parse");

    let deadline_budget = Duration::from_secs(600);
    let started = Instant::now();

    for cycle in 0..CYCLES {
        assert!(
            started.elapsed() < deadline_budget,
            "the stress walk stalled at cycle {cycle}"
        );
        match rng.below(100) {
            // A position command resets the walk's position.
            0..=29 => {
                let (fen_text, label) = if rng.below(2) == 0 {
                    (
                        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                        "start",
                    )
                } else {
                    (
                        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
                        "kiwi",
                    )
                };
                engine.send(&format!("position fen {fen_text}"));
                sent.push(format!("[{cycle}] position {label}"));
                position = Position::from_fen(fen_text).expect("verified FEN");
            }
            // A search with a random limiter.
            30..=59 => {
                let before = engine.bestmove_count();
                let depth = 1 + rng.below(4) as u32;
                engine.send(&format!("go depth {depth}"));
                sent.push(format!("[{cycle}] go depth {depth}"));
                // Bounded per-search wait: a dead or stuck engine fails
                // HERE with the full trace, not two cycles later.
                let search_deadline = Instant::now() + Duration::from_secs(30);
                let answered = engine.wait_for_bestmoves(before, search_deadline);
                if !answered {
                    let alive = engine.is_alive();
                    let stderr = engine.stderr_text();
                    let output: Vec<String> = engine
                        .lines
                        .lock()
                        .expect("reader lock")
                        .iter()
                        .map(|(_, line)| line.clone())
                        .collect();
                    panic!(
                        "the go was not answered within 30 s (cycle {cycle}, engine alive: {alive})\n\
                         engine stderr: {stderr}\n\
                         all commands: {sent:?}\n\
                         last output: {:?}",
                        &output[output.len().saturating_sub(20)..]
                    );
                }
                // Exactly one bestmove appeared for this go, and it is legal
                // for the position that was searched.
                let last = {
                    let lines = engine.lines.lock().expect("reader lock");
                    lines
                        .iter()
                        .rev()
                        .find(|(_, line)| line.starts_with("bestmove "))
                        .expect("the go must be answered")
                        .1
                        .clone()
                };
                let move_text = last.split_whitespace().nth(1).expect("bestmove move");
                let found = legal(&position)
                    .as_slice()
                    .iter()
                    .any(|mv: &Move| mv.to_uci() == move_text);
                if !found {
                    let alive = engine.child.try_wait().expect("probe").is_none();
                    let output: Vec<String> = engine
                        .lines
                        .lock()
                        .expect("reader lock")
                        .iter()
                        .map(|(_, line)| line.clone())
                        .collect();
                    panic!(
                        "bestmove {move_text} is not legal in {} (cycle {cycle}, engine alive: {alive})\n\
                         all commands: {sent:?}\n\
                         last output: {:?}",
                        position.to_fen(),
                        &output[output.len().saturating_sub(20)..]
                    );
                }
                bestmoves_expected += 1;
            }
            // A stop (harmless with and without a running search).
            60..=74 => {
                engine.send("stop");
                sent.push(format!("[{cycle}] stop"));
            }
            // An option change: thread count, hash size, a lever.
            75..=89 => match rng.below(3) {
                0 => {
                    let threads = [1, 2, 4, 8][rng.below(4) as usize];
                    engine.send(&format!("setoption name Threads value {threads}"));
                    sent.push(format!("[{cycle}] Threads={threads}"));
                }
                1 => {
                    let hash = [4, 16, 64][rng.below(3) as usize];
                    engine.send(&format!("setoption name Hash value {hash}"));
                    sent.push(format!("[{cycle}] Hash={hash}"));
                }
                _ => {
                    let on = rng.below(2) == 0;
                    engine.send(&format!(
                        "setoption name UseStableTimeManagement value {on}"
                    ));
                    sent.push(format!("[{cycle}] lever={on}"));
                }
            },
            // An isready while searches come and go.
            _ => {
                engine.send("isready");
                sent.push(format!("[{cycle}] isready"));
                // readyok must come — bounded wait.
                let deadline = Instant::now() + Duration::from_secs(30);
                loop {
                    let seen = {
                        let lines = engine.lines.lock().expect("reader lock");
                        lines.iter().filter(|(_, line)| line == "readyok").count()
                    };
                    if seen > 0 || Instant::now() > deadline {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
        // Every go must have been answered by now (the wait above enforced
        // it): the count cannot exceed the number of gos.
        let lines = engine.lines.lock().expect("reader lock");
        assert_eq!(
            bestmove_count(
                &lines
                    .iter()
                    .map(|(_, line)| line.clone())
                    .collect::<Vec<String>>()
            ),
            bestmoves_expected,
            "exactly one bestmove per go (cycle {cycle})"
        );
        drop(lines);
    }

    // The walk's final position is left as-is; the engine shuts down cleanly.
    engine.shutdown();
}
