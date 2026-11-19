//! Phase 5 step 12: UCI transcript tests against the step-0 captures.
//!
//! Every published transcript in `tests/golden/transcripts/` is replayed
//! against the real engine binary. The comparison is deliberately about
//! *protocol structure*, not about search values: timing fields (`time`,
//! `nps`, `hashfull`), score and node values and PV tails are masked, because
//! they depend on host speed and on the reference's derived PV reporting.
//! What is checked is the order and shape of every reply, the public option
//! set, the number of `bestmove` answers, their legality, and the cold
//! fixed-depth node pins (the bench gate, not the warm-TT transcript values).
//!
//! The test FENs are verified against the configured external verifier at
//! test time, like everywhere else in the project.

mod common;

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use knightbyte::board::movegen::legal;
use knightbyte::board::position::Position;
use knightbyte::types::r#move::Move;

/// Directory holding the published step-0 transcripts.
const TRANSCRIPT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/transcripts");
/// The start position, used for legality checks.
const STARTPOS_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
/// Back-rank mate-in-one fixture from the step-0 capture.
const MATE_IN_ONE_FEN: &str = "6k1/5ppp/8/8/8/8/5PPP/R5K1 w - - 0 1";

/// A live engine with background stdout/stderr readers.
///
/// Line readers are required because the UCI loop answers `isready` and
/// `stop` while a search is running; a burst `write_all` + `wait_with_output`
/// would hide that interleaving (and can stop a search before it starts).
struct Session {
    child: Child,
    stdin: ChildStdin,
    lines: Arc<Mutex<Vec<String>>>,
    stderr: Arc<Mutex<String>>,
    reader: Option<JoinHandle<()>>,
    err_reader: Option<JoinHandle<()>>,
}

impl Session {
    /// Spawns the engine and starts both line readers.
    fn new() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_knightbyte"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn knightbyte");
        let stdin = child.stdin.take().expect("stdin");

        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&lines);
        let stdout = child.stdout.take().expect("stdout");
        let reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                sink.lock().expect("line sink").push(line);
            }
        });

        let stderr = Arc::new(Mutex::new(String::new()));
        let err_sink = Arc::clone(&stderr);
        let mut err_pipe = child.stderr.take().expect("stderr");
        let err_reader = thread::spawn(move || {
            let mut text = String::new();
            let _ = err_pipe.read_to_string(&mut text);
            *err_sink.lock().expect("stderr sink") = text;
        });

        Self {
            child,
            stdin,
            lines,
            stderr,
            reader: Some(reader),
            err_reader: Some(err_reader),
        }
    }

    /// Sends one UCI command line.
    fn send(&mut self, line: &str) {
        writeln!(self.stdin, "{line}").expect("write UCI command");
        self.stdin.flush().expect("flush UCI command");
    }

    /// Number of collected lines matching `prefix`.
    fn count(&self, prefix: &str) -> usize {
        self.lines
            .lock()
            .expect("lines")
            .iter()
            .filter(|line| line.starts_with(prefix))
            .count()
    }

    /// Waits until at least `wanted` lines with `prefix` have arrived.
    ///
    /// A dead engine fails immediately with its stderr and the last output
    /// lines, so a transcript failure names the offending command.
    fn wait_for(&mut self, prefix: &str, wanted: usize, timeout: Duration) {
        let started = Instant::now();
        loop {
            if self.count(prefix) >= wanted {
                return;
            }
            if self.child.try_wait().expect("probe engine").is_some() {
                panic!(
                    "engine exited before {wanted} x {prefix:?}: stderr={:?}\n{}",
                    self.stderr.lock().expect("stderr"),
                    self.tail()
                );
            }
            if started.elapsed() > timeout {
                panic!("timeout waiting for {wanted} x {prefix:?}\n{}", self.tail());
            }
            thread::sleep(Duration::from_millis(2));
        }
    }

    /// Waits until no new line has arrived for `quiet`.
    fn wait_idle(&mut self, quiet: Duration, timeout: Duration) {
        let started = Instant::now();
        let mut last_change = Instant::now();
        let mut last_count = self.lines.lock().expect("lines").len();
        loop {
            let count = self.lines.lock().expect("lines").len();
            if count != last_count {
                last_count = count;
                last_change = Instant::now();
            }
            if last_change.elapsed() >= quiet {
                return;
            }
            if self.child.try_wait().expect("probe engine").is_some() {
                panic!(
                    "engine exited while waiting for silence: stderr={:?}\n{}",
                    self.stderr.lock().expect("stderr"),
                    self.tail()
                );
            }
            if started.elapsed() > timeout {
                panic!("timeout waiting for silence\n{}", self.tail());
            }
            thread::sleep(Duration::from_millis(2));
        }
    }

    /// Waits for one more `bestmove` after `before` were seen.
    fn wait_bestmove(&mut self, before: usize) {
        self.wait_for("bestmove ", before + 1, Duration::from_secs(60));
    }

    /// Waits for one more `readyok` after `before` were seen.
    fn wait_readyok(&mut self, before: usize) {
        self.wait_for("readyok", before + 1, Duration::from_secs(10));
    }

    /// Returns all output collected so far.
    fn snapshot(&self) -> Vec<String> {
        self.lines.lock().expect("lines").clone()
    }

    /// Last 30 lines, for failure messages.
    fn tail(&self) -> String {
        let lines = self.snapshot();
        let start = lines.len().saturating_sub(30);
        lines[start..].join("\n")
    }

    /// Ends the engine cleanly and returns its full output.
    fn finish(mut self) -> Vec<String> {
        // `quit` is harmless if the engine already exited; try_wait in the
        // wait helpers would have panicked on an unexpected early exit.
        let _ = writeln!(self.stdin, "quit");
        let _ = self.stdin.flush();
        let status = self.child.wait().expect("wait for engine");
        self.reader
            .take()
            .expect("reader")
            .join()
            .expect("reader join");
        self.err_reader
            .take()
            .expect("stderr reader")
            .join()
            .expect("stderr join");
        let stderr = self.stderr.lock().expect("stderr").clone();
        assert!(status.success(), "engine failed ({status}): {stderr}");
        assert!(stderr.is_empty(), "engine wrote to stderr: {stderr}");
        self.snapshot()
    }
}

/// Runs one short burst script and returns all output (for the fixed-depth
/// node pins and the bench transcript, neither of which needs live timing).
fn run_burst(commands: &str, wait_for: &str) -> Vec<String> {
    let mut engine = Session::new();
    engine.send(commands);
    engine.wait_for(wait_for, 1, Duration::from_secs(60));
    engine.finish()
}

/// Maps a transcript line to a comparable protocol shape.
///
/// Values that depend on wall-clock time, host speed or derived reporting
/// (depth/score/node/nps/hashfull/time numbers and PV moves) become
/// placeholders; field names, score kinds and line kinds stay exact.
fn normalize(line: &str) -> Option<String> {
    let line = line.trim_end();
    if line.is_empty() || line.starts_with('#') || line.starts_with("> ") {
        return None;
    }
    if let Some(rest) = line.strip_prefix("info depth ") {
        let (head, pv) = rest.split_once(" pv ").unwrap_or((rest, ""));
        let tokens: Vec<String> = head
            .split_whitespace()
            .map(|token| {
                if token.starts_with('<') || token.parse::<i64>().is_ok() {
                    "<n>".to_string()
                } else {
                    token.to_string()
                }
            })
            .collect();
        let mut shape = format!("info depth {}", tokens.join(" "));
        if !pv.is_empty() {
            shape.push_str(" pv <pv>");
        }
        return Some(shape);
    }
    if line.starts_with("info currmove ") {
        return Some("info currmove <m> currmovenumber <n> nodes <n>".to_string());
    }
    if let Some(mv) = line.strip_prefix("bestmove ") {
        return Some(if mv == "0000" {
            "bestmove 0000".to_string()
        } else {
            "bestmove <m>".to_string()
        });
    }
    if line.starts_with("Nodes:") {
        return Some("Nodes: <n>".to_string());
    }
    if line.starts_with("Time:") {
        return Some("Time:".to_string());
    }
    if line.starts_with("NPS:") {
        return Some("NPS:".to_string());
    }
    if line.starts_with("info string eval-source:") {
        return Some("info string eval-source: <net>".to_string());
    }
    if line.starts_with("id name ") {
        return Some("id name <engine>".to_string());
    }
    if line.starts_with("id author ") {
        return Some("id author <author>".to_string());
    }
    Some(line.to_string())
}

/// Reads a golden transcript and reduces it to comparable shapes.
fn golden(name: &str) -> Vec<String> {
    let path = Path::new(TRANSCRIPT_DIR).join(format!("{name}.txt"));
    let text = fs::read_to_string(&path).expect("read golden transcript");
    text.lines().filter_map(normalize).collect()
}

/// Collapses each search's report block into one shape.
///
/// A search reports a host-dependent number of `info depth` lines and may or
/// may not reach a later root move (`info currmove`). Their relative order to
/// `bestmove` is what the transcript pins, so both engines' report blocks
/// reduce to one token per search; the field shapes themselves are still
/// checked when the line is first normalized.
fn compress(lines: &[String]) -> Vec<String> {
    const REPORT: &str = "info <search report>";
    let mut out: Vec<String> = Vec::new();
    for line in lines {
        if line.starts_with("info depth ") || line.starts_with("info currmove ") {
            if out.last().map(String::as_str) != Some(REPORT) {
                out.push(REPORT.to_string());
            }
        } else if out.last() != Some(line) {
            out.push(line.clone());
        }
    }
    out
}

/// Checks that every expected line occurs in order in the actual output.
fn assert_transcript(name: &str, actual: &[String]) {
    let expected = golden(name);
    let mut actual_shapes: Vec<String> = actual.iter().filter_map(|line| normalize(line)).collect();
    if name == "handshake" {
        // The research levers are KnightByte's documented option superset
        // (step 2); the public option set itself must match exactly.
        let public: Vec<String> = expected
            .iter()
            .filter(|line| line.starts_with("option name "))
            .cloned()
            .collect();
        actual_shapes.retain(|line| !line.starts_with("option name ") || public.contains(line));
    }

    let expected_bestmoves = expected
        .iter()
        .filter(|l| l.starts_with("bestmove "))
        .count();
    let actual_bestmoves = actual_shapes
        .iter()
        .filter(|l| l.starts_with("bestmove "))
        .count();
    assert_eq!(
        actual_bestmoves,
        expected_bestmoves,
        "{name}: wrong number of bestmove answers\n{}",
        actual.join("\n")
    );

    let expected = compress(&expected);
    let actual_shapes = compress(&actual_shapes);
    let mut cursor = 0;
    for want in &expected {
        let found = actual_shapes[cursor..].iter().position(|line| line == want);
        match found {
            Some(offset) => cursor += offset + 1,
            None => panic!(
                "{name}: missing {want:?} after {cursor} expected lines\nactual output:\n{}",
                actual.join("\n")
            ),
        }
    }
    if golden_raw_has_currmove(name) {
        assert!(
            actual.iter().any(|line| line.starts_with("info currmove ")),
            "{name}: no currmove line at all\n{}",
            actual.join("\n")
        );
    }
}

/// Whether the captured transcript contains a currmove report line.
fn golden_raw_has_currmove(name: &str) -> bool {
    let path = Path::new(TRANSCRIPT_DIR).join(format!("{name}.txt"));
    fs::read_to_string(path)
        .expect("read golden transcript")
        .lines()
        .any(|line| line.starts_with("info currmove "))
}

/// Checks every non-null bestmove for legality in one of `fens`.
///
/// Each FEN constant is verified against the external verifier at test time
/// (RULE HARDENING); positions derived from a verified FEN by a legal move use
/// [`assert_bestmoves_legal_in`] instead, so the verifier's different
/// en-passant normalization does not reject them.
fn assert_bestmoves_legal(lines: &[String], fens: &[&str]) {
    let mut positions = Vec::new();
    for fen in fens {
        common::verify_fen(fen);
        positions.push(Position::from_fen(fen).expect("verified FEN"));
    }
    assert_bestmoves_legal_in(lines, &positions);
}

/// Legality check against positions that are already known to be legal.
fn assert_bestmoves_legal_in(lines: &[String], positions: &[Position]) {
    for line in lines.iter().filter(|line| line.starts_with("bestmove ")) {
        let mv = line.split_whitespace().nth(1).expect("bestmove move");
        if mv == "0000" {
            continue;
        }
        let legal_somewhere = positions.iter().any(|position| {
            legal(position)
                .as_slice()
                .iter()
                .any(|candidate: &Move| candidate.to_uci() == mv)
        });
        assert!(legal_somewhere, "illegal bestmove {line:?}");
    }
}

/// Returns the node count reported at `depth` in the last matching line.
fn nodes_at_depth(lines: &[String], depth: u32) -> u64 {
    let prefix = format!("info depth {depth} ");
    lines
        .iter()
        .rev()
        .find(|line| line.starts_with(&prefix))
        .and_then(|line| line.split_whitespace().skip_while(|t| *t != "nodes").nth(1))
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("no nodes reported at depth {depth}"))
}

#[test]
fn handshake_transcript_matches_step0() {
    let mut engine = Session::new();
    engine.wait_idle(Duration::from_millis(50), Duration::from_secs(5)); // banner
    engine.send("uci");
    engine.wait_for("uciok", 1, Duration::from_secs(5));
    let before = engine.count("readyok");
    engine.send("isready");
    engine.wait_readyok(before);
    engine.send("ucinewgame");
    engine.wait_idle(Duration::from_millis(50), Duration::from_secs(5));
    let before = engine.count("readyok");
    engine.send("isready");
    engine.wait_readyok(before);
    let lines = engine.finish();
    assert_transcript("handshake", &lines);
}

#[test]
fn go_limiter_depth_transcript_matches_step0() {
    let mut engine = Session::new();
    engine.send("position startpos\n");
    // The warm node counts are pinned as well (B41): a repeated search over
    // the same position must reproduce the reference's warm table reuse.
    let mut warm_nodes = Vec::new();
    for command in ["go", "go depth 7", "go nodes 50000", "go movestogo 40"] {
        let before = engine.count("bestmove ");
        engine.send(command);
        engine.wait_bestmove(before);
        let snapshot = engine.snapshot();
        let last = snapshot
            .iter()
            .rev()
            .find(|line| line.starts_with("info depth "))
            .expect("depth line");
        let nodes = last
            .split_whitespace()
            .skip_while(|token| *token != "nodes")
            .nth(1)
            .and_then(|value| value.parse::<u64>().ok())
            .expect("nodes in the last depth line");
        warm_nodes.push(nodes);
    }
    engine.send("go perft 3");
    engine.wait_for("Nodes:", 1, Duration::from_secs(10));
    engine.wait_idle(Duration::from_millis(50), Duration::from_secs(5));
    let lines = engine.finish();
    assert_transcript("go_limiter_depth", &lines);
    assert_eq!(
        warm_nodes,
        vec![5741, 5731, 34979, 169],
        "warm node counts: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l == "Nodes: 8902"),
        "perft count: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("Time:")),
        "perft Time line"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("NPS:")),
        "perft NPS line"
    );
    assert_bestmoves_legal(&lines, &[STARTPOS_FEN]);
}

#[test]
fn go_limiter_time_transcript_matches_step0() {
    let mut engine = Session::new();
    engine.send("position startpos\n");
    for command in [
        "go movetime 250",
        "go wtime 1000 btime 1000 winc 10 binc 10",
        "go wtime 1000 btime 1000 winc 10 binc 10 movestogo 20",
    ] {
        let before = engine.count("bestmove ");
        engine.send(command);
        engine.wait_bestmove(before);
    }
    let lines = engine.finish();
    assert_transcript("go_limiter_time", &lines);
    assert_bestmoves_legal(&lines, &[STARTPOS_FEN]);
}

#[test]
fn go_infinite_stop_transcript_matches_step0() {
    let mut engine = Session::new();
    engine.send("position startpos\n");
    engine.send("go infinite");
    engine.wait_for("info depth ", 2, Duration::from_secs(10));
    engine.send("stop");
    engine.wait_bestmove(0);
    let lines = engine.finish();
    assert_transcript("go_infinite_stop", &lines);
    assert_bestmoves_legal(&lines, &[STARTPOS_FEN]);
}

#[test]
fn stop_transcript_matches_step0() {
    let mut engine = Session::new();
    engine.send("position startpos\n");
    engine.send("stop"); // a) no search running: silent
    engine.wait_idle(Duration::from_millis(50), Duration::from_secs(5));

    engine.send("go depth 1"); // b) burst stop during depth 1
    engine.send("stop");
    engine.wait_bestmove(0);

    engine.send("go depth 14"); // c) stop during a deep search
    engine.wait_for("info depth ", 2, Duration::from_secs(10));
    engine.send("stop");
    engine.wait_bestmove(1);

    engine.send("go depth 5"); // d) stop after the search finished
    engine.wait_bestmove(2);
    engine.send("stop");
    engine.wait_idle(Duration::from_millis(50), Duration::from_secs(5));

    let lines = engine.finish();
    assert_transcript("stop", &lines);
    assert_bestmoves_legal(&lines, &[STARTPOS_FEN]);
}

#[test]
fn ponder_transcript_matches_step0() {
    let mut engine = Session::new();
    engine.send("position startpos\n");
    engine.send("go ponder wtime 5000 btime 5000 winc 100 binc 100");
    engine.wait_for("info depth ", 2, Duration::from_secs(10));
    engine.send("stop");
    engine.wait_bestmove(0); // Q5 fix: the best move so far, not 0000

    engine.send("go ponder wtime 5000 btime 5000 winc 100 binc 100");
    // Wait for a NEW iteration: counting from zero would be satisfied by the
    // previous search's lines and ponderhit would arrive before iteration 1.
    let before_depth = engine.count("info depth ");
    engine.wait_for("info depth ", before_depth + 1, Duration::from_secs(10));
    let before_ponderhit = engine.snapshot();
    engine.send("ponderhit");
    engine.wait_bestmove(1);

    engine.send("ponderhit"); // c) stray ponderhit: silent
    engine.wait_idle(Duration::from_millis(50), Duration::from_secs(5));
    let lines = engine.finish();

    // The ponder move is the first PV move of the last ponder iteration; it
    // is applied to the root by ponderhit, so the second bestmove is legal
    // in that child position.
    let ponder_move = before_ponderhit
        .iter()
        .rev()
        .find(|line| line.starts_with("info depth "))
        .and_then(|line| {
            line.split_once(" pv ")
                .map(|(_, pv)| pv.split_whitespace().next())
        })
        .flatten()
        .expect("ponder PV");
    common::verify_fen(STARTPOS_FEN);
    let startpos = Position::from_fen(STARTPOS_FEN).expect("startpos");
    let child = legal(&startpos)
        .as_slice()
        .iter()
        .find(|mv: &&Move| mv.to_uci() == ponder_move)
        .map(|mv| startpos.make_move(*mv))
        .expect("ponder move must be legal");
    assert_transcript("ponder", &lines);
    assert_bestmoves_legal_in(&lines, &[startpos, child]);
}

#[test]
fn mate_searchmoves_transcript_matches_step0() {
    common::verify_fen(MATE_IN_ONE_FEN);
    let mut engine = Session::new();
    engine.send(&format!("position fen {MATE_IN_ONE_FEN}\n"));
    engine.send("go mate 1");
    engine.wait_bestmove(0);
    let mate_nodes = nodes_at_depth(&engine.snapshot(), 1);

    engine.send("position startpos\n");
    engine.send("go mate 3"); // unbounded: stop after a bounded window
    thread::sleep(Duration::from_millis(400));
    if engine.count("bestmove ") == 1 {
        engine.send("stop");
        engine.wait_bestmove(1);
    }

    engine.send("position startpos\n");
    for command in [
        "go depth 5 searchmoves e2e4",
        "go depth 5 searchmoves e2e4 d2d4",
        "go depth 5 searchmoves",
    ] {
        let before = engine.count("bestmove ");
        engine.send(command);
        engine.wait_bestmove(before);
    }
    let lines = engine.finish();
    assert_transcript("mate_searchmoves", &lines);
    assert_eq!(mate_nodes, 43, "mate-in-one pin");
    assert_bestmoves_legal(&lines, &[STARTPOS_FEN, MATE_IN_ONE_FEN]);
}

#[test]
fn ucinewgame_transcript_matches_step0() {
    let mut engine = Session::new();
    engine.send("position startpos moves e2e4\nucinewgame\n");
    engine.wait_idle(Duration::from_millis(50), Duration::from_secs(5));
    engine.send("go depth 1");
    engine.wait_bestmove(0);
    let lines = engine.finish();
    assert_transcript("ucinewgame", &lines);
    assert_eq!(nodes_at_depth(&lines, 1), 47, "cold depth-1 pin");
    assert_bestmoves_legal(&lines, &[STARTPOS_FEN]);
}

#[test]
fn setoption_transcript_matches_step0() {
    let mut engine = Session::new();
    for command in [
        "setoption name Hash value 32",
        "setoption name Hash value 999999",
        "setoption name Hash value 0",
        "setoption name NoSuchOption value 5",
        "setoption name Own Book value true",
        "setoption name Own Book value false",
        "setoption name Own Book value maybe",
        "setoption name Clear Hash",
        "setoption name Threads value 0",
        "setoption name Threads value 999",
    ] {
        engine.send(command);
        engine.wait_idle(Duration::from_millis(40), Duration::from_secs(5));
    }
    let lines = engine.finish();
    assert_transcript("setoption", &lines);
}

#[test]
fn isready_during_search_transcript_matches_step0() {
    let mut engine = Session::new();
    engine.send("position startpos\n");
    engine.send("go depth 16");
    engine.wait_for("info depth ", 2, Duration::from_secs(10));
    let before = engine.count("readyok");
    engine.send("isready");
    engine.wait_readyok(before);
    engine.send("stop");
    engine.wait_bestmove(0);
    let lines = engine.finish();
    assert_transcript("isready_during_search", &lines);
    assert_bestmoves_legal(&lines, &[STARTPOS_FEN]);
}

#[test]
fn threads_transcript_matches_step0() {
    // The captured scenario uses depth 12, but this suite runs on the debug
    // test binary (~100x slower than release). A short movetime exercises the
    // same report structure (one reporting thread, one bestmove per search)
    // without a multi-minute test; node counts are the bench gate's job.
    let mut engine = Session::new();
    engine.send("position startpos\n");
    engine.send("go movetime 200");
    engine.wait_bestmove(0);
    engine.send("setoption name Threads value 4");
    engine.wait_idle(Duration::from_millis(50), Duration::from_secs(5));
    engine.send("go movetime 200");
    engine.wait_bestmove(1);
    engine.send("setoption name Threads value 1");
    engine.wait_idle(Duration::from_millis(50), Duration::from_secs(5));
    let lines = engine.finish();
    assert_transcript("threads", &lines);
    assert_bestmoves_legal(&lines, &[STARTPOS_FEN]);
}

#[test]
fn bench_transcript_matches_expected_structure() {
    let lines = run_burst("bench 6\n", "NPS:");
    let expected = [
        "info string bench positions 4 depth 6",
        "info string bench position 1 nodes 5741 bestmove e2e4",
        "info string bench position 2 nodes 2683 bestmove e2a6",
        "info string bench position 3 nodes 2019 bestmove b4f4",
        "info string bench position 4 nodes 5085 bestmove c4c5",
        "Nodes: <n>",
        "Time:",
        "NPS:",
    ];
    let shapes: Vec<String> = lines.iter().filter_map(|line| normalize(line)).collect();
    let mut cursor = 0;
    for want in expected {
        let found = shapes[cursor..].iter().position(|line| line == want);
        assert!(
            found.is_some(),
            "bench transcript missing {want:?}\n{}",
            lines.join("\n")
        );
        cursor += found.unwrap() + 1;
    }
    assert!(
        lines.iter().any(|line| line == "Nodes: 15528"),
        "bench 6 total node pin: {lines:?}"
    );
}
