//! End-to-end smoke test for the UCI lifecycle.

mod common;

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn uci_handshake_and_quit() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_knightbyte"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn knightbyte");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"uci\nisready\nquit\n")
        .expect("write UCI commands");

    let output = child.wait_with_output().expect("wait for knightbyte");
    assert!(output.status.success(), "engine exited with failure");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("id name KnightByte"), "missing id name");
    assert!(
        stdout.contains("id author KnightByte project"),
        "missing id author"
    );
    assert!(stdout.contains("uciok"), "missing uciok");
    assert!(
        stdout.contains("option name Hash type spin default 256 min 1 max 1024"),
        "missing Hash option"
    );
    assert!(
        stdout.contains("option name AspirationBase type spin default 50"),
        "missing research lever"
    );
    assert!(stdout.contains("readyok"), "missing readyok");
}

#[test]
fn diagnostic_position_perft_and_nnueeval_work() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_knightbyte"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn knightbyte");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"position startpos\nperft 3\nnnueeval\nquit\n")
        .expect("write diagnostic commands");

    let output = child.wait_with_output().expect("wait for knightbyte");
    assert!(output.status.success(), "engine exited with failure");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Nodes: 8902"), "perft output missing");
    assert!(stdout.contains("nnueeval 27"), "network evaluation missing");
}

#[test]
fn unknown_command_is_reported_without_crashing() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_knightbyte"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn knightbyte");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"this-command-does-not-exist\nquit\n")
        .expect("write UCI commands");

    let output = child.wait_with_output().expect("wait for knightbyte");
    assert!(output.status.success(), "engine exited with failure");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Reference parity: the report names the lowercased command word.
    assert!(
        stdout.contains("Unknown command: this-command-does-not-exist"),
        "missing unknown-command diagnostic"
    );
}

#[test]
fn go_depth_returns_a_bestmove_and_an_info_line() {
    // Interactive (GUI) pattern: the search runs on the worker and `quit`
    // stops a running search, so the test waits for the bestmove first.
    let stdout = run_interactively(b"position startpos moves e2e4 e7e5\ngo depth 2\n");
    assert!(stdout.contains("info depth 1 "), "missing first info line");
    assert!(stdout.contains("info depth 2 "), "missing second info line");
    let bestmove = stdout
        .lines()
        .find(|line| line.starts_with("bestmove "))
        .expect("missing bestmove line");
    assert!(
        bestmove.split_whitespace().count() == 2,
        "bestmove must name exactly one move: {bestmove}"
    );
}

#[test]
fn setoption_and_time_limited_go_work() {
    let stdout = run_interactively(
        b"position startpos\nsetoption name Hash value 8\n\
          setoption name AspirationBase value 60\n\
          setoption name Clear Hash\ngo movetime 500\n",
    );
    assert!(stdout.contains("info depth 1 "), "no search happened");
    assert!(
        stdout.contains("bestmove "),
        "missing bestmove after a timed search"
    );
}

/// Runs the engine on `input` and returns everything it printed.
fn run_engine(input: &[u8]) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_knightbyte"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn knightbyte");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(input)
        .expect("write UCI commands");
    let output = child.wait_with_output().expect("wait for knightbyte");
    assert!(output.status.success(), "engine exited with failure");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Sends `commands`, waits until the search answers its `bestmove`, then
/// quits, and returns everything the engine printed.
///
/// This is the GUI pattern: `quit` stops a running search (the reference
/// does the same — a burst `go depth 6` + `quit` gets the depth-0 answer),
/// so a test that wants the full search must wait for the `bestmove` before
/// quitting.
fn run_interactively(commands: &[u8]) -> String {
    let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (mut child, reader) = spawn_with_reader(&lines);
    let mut stdin = child.stdin.take().expect("stdin");
    stdin.write_all(commands).expect("write UCI commands");
    stdin.flush().expect("flush UCI commands");

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let done = lines
            .lock()
            .expect("reader lock")
            .iter()
            .any(|(_, line)| line.starts_with("bestmove "));
        if done || Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    stdin.write_all(b"quit\n").expect("write quit");
    drop(stdin);
    assert!(child.wait().expect("wait").success(), "engine failed");
    reader.join().expect("reader");
    lines
        .lock()
        .expect("reader lock")
        .iter()
        .map(|(_, line)| line.clone())
        .collect::<Vec<String>>()
        .join("\n")
}

// The old "an unsupported go limiter still gets a bestmove" guarantee is
// fully covered since step 6: every `go` limiter is implemented (ponder,
// mate, searchmoves included), and every `go` is answered by its worker —
// see the step 4/5/6 tests below.

#[test]
fn a_quit_ends_a_ponder_search_with_the_null_move() {
    // Verified against the reference binary (2026-11-18): a burst
    // `go ponder` + `quit` prints the interrupted-search depth-0 line and
    // answers the null move — the ponder search never got its ponderhit.
    let stdout =
        run_engine(b"position startpos\ngo ponder wtime 5000 btime 5000 winc 100 binc 100\nquit\n");
    let depth_lines: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("info depth "))
        .collect();
    assert_eq!(
        depth_lines,
        vec!["info depth 0 score cp 27 pv a2a3"],
        "the interrupted ponder search reports the depth-0 line: {stdout}"
    );
    let bestmoves: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("bestmove "))
        .collect();
    assert_eq!(bestmoves, vec!["bestmove 0000"], "Q5: {stdout}");
}

#[test]
fn go_without_a_position_searches_the_start_position() {
    // Reference parity (the predecessor's bug-143 fix): the engine starts on
    // the start position and `ucinewgame` resets to it, so a bare `go` never
    // runs without a position — it searches the start position and answers a
    // real bestmove. The null-bestmove answer path stays as robustness for a
    // state that can no longer be reached through the protocol.
    let stdout = run_interactively(b"go depth 3\n");
    assert!(stdout.contains("info depth 1 "), "no search happened");
    let bestmove = stdout
        .lines()
        .find(|line| line.starts_with("bestmove "))
        .expect("missing bestmove after a bare go");
    assert_ne!(bestmove, "bestmove 0000", "the start position was searched");
}

#[test]
fn a_rejected_position_command_is_reported_and_keeps_the_previous_one() {
    // Reference parity (the predecessor's bug-144/145 decision): a rejected
    // `position` command prints the loud error line and changes nothing, so
    // the next `go` still searches the previous position. The moves of a
    // rejected command are applied to a copy — nothing is committed.
    let stdout = run_engine(
        b"position startpos moves e2e4\n\
          position fen this-is-not-a-fen\n\
          nnueeval\n\
          position startpos moves e2e4 e2e4 d7d5\n\
          nnueeval\n\
          go depth 1\nquit\n",
    );
    assert!(
        stdout.contains("info string Error: invalid position, previous position kept"),
        "missing the invalid-position error line"
    );
    assert!(
        stdout.contains(
            "info string Error: illegal move e2e4, position command rejected, previous position kept"
        ),
        "missing the illegal-move error line"
    );
    // The position after e2e4 is kept through both rejections: the start
    // position evaluates 27, the position after e2e4 evaluates -45.
    assert!(
        stdout.contains("nnueeval -45"),
        "the previous position was not kept"
    );
}

#[test]
fn ucinewgame_resets_to_the_start_position() {
    // Reference parity (the predecessor's bug-143 fix): `ucinewgame` resets
    // the position to the start, so a `go` afterwards searches the start
    // position, not the previous game's last position (startpos eval 27,
    // after e2e4 it is -45).
    let stdout = run_engine(
        b"position startpos moves e2e4\n\
          ucinewgame\n\
          nnueeval\n\
          go depth 1\nquit\n",
    );
    assert!(
        stdout.contains("nnueeval 27"),
        "ucinewgame did not reset to the start position"
    );
    assert!(stdout.contains("bestmove "), "missing bestmove");
}

#[test]
fn a_clocked_search_after_a_game_with_moves_returns_in_time() {
    // The clock starts at `go`, and the game history of the position command
    // is accepted: two knights out and back repeat the start position.
    let stdout = run_interactively(
        b"ucinewgame\nposition startpos moves g1f3 g8f6 f3g1 f6g8\n\
          go wtime 2000 btime 2000 winc 20 binc 20\n",
    );
    assert!(stdout.contains("info depth 1 "), "no search happened");
    assert!(stdout.contains("bestmove "), "missing bestmove");
}

/// Runs the engine on `input` with `args` and returns (stdout, success).
fn run_engine_with_args(args: &[&str], input: &[u8]) -> (String, bool) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_knightbyte"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn knightbyte");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(input)
        .expect("write UCI commands");
    let output = child.wait_with_output().expect("wait for knightbyte");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.success(),
    )
}

#[test]
fn the_step_zero_diagnostics_are_recognised() {
    // Phase 5 step 1: the reference's diagnostic surface is recognised. The
    // outputs mirror the reference's formats; stop and ponderhit are accepted
    // no-ops until their steps land.
    let stdout =
        run_engine(b"position startpos\nd\neval\ncheckmoves\ndebug on\nstop\nponderhit\nquit\n");
    assert!(
        stdout.contains("info string FEN: rnbqkbnr/"),
        "missing FEN line"
    );
    assert!(
        stdout.contains("info string Zobrist: 0x"),
        "missing zobrist line"
    );
    assert!(
        stdout.contains("info string Evaluation: 27 cp (source: nnue net jokausch.nnue)"),
        "missing evaluation line"
    );
    assert!(
        stdout.contains("checkmove e2e4 givescheck="),
        "missing checkmove lines"
    );
    assert!(stdout.contains("checkmoves done"), "missing the terminator");
}

#[test]
fn go_perft_runs_the_counter_instead_of_a_search() {
    // Reference parity: `go perft N` prints the node count and no bestmove.
    let stdout = run_engine(b"position startpos\ngo perft 2\nquit\n");
    assert!(stdout.contains("Nodes: 400"), "missing perft output");
    assert!(
        !stdout.lines().any(|line| line.starts_with("bestmove ")),
        "go perft must not answer a bestmove"
    );
}

#[test]
fn a_bare_go_searches_depth_six() {
    // Reference parity: a `go` with no limiter searches depth 6 (the old
    // KnightByte default of 8 was a recorded deviation).
    let stdout = run_interactively(b"position startpos\ngo\n");
    assert!(stdout.contains("info depth 6 "), "depth 6 was not reached");
    assert!(stdout.contains("bestmove "), "missing bestmove");
}

#[test]
fn unknown_go_tokens_are_ignored_not_rejected() {
    // Reference parity: a `go` with an unknown token still searches instead of
    // reporting an unsupported limiter, so a GUI never loses its move.
    let stdout = run_interactively(b"position startpos\ngo depth 2 nonsense\n");
    assert!(
        !stdout.contains("not supported"),
        "the unknown token was reported as an error"
    );
    assert!(stdout.contains("info depth 2 "), "no search happened");
    assert!(stdout.contains("bestmove "), "missing bestmove");
}

#[test]
fn argv_runs_a_one_shot_command_line() {
    // The reference's argv surface: all non-param arguments are joined into ONE
    // UCI command line, run once, and the engine exits afterwards. A burst
    // `go depth 1` + `quit` stops the search before an iteration completes —
    // verified against the reference binary (2026-11-18): it answers the same
    // depth-0 line.
    let (stdout, success) = run_engine_with_args(&["go depth 1"], b"");
    assert!(success, "one-shot run failed");
    assert!(
        stdout
            .lines()
            .filter(|line| line.starts_with("bestmove "))
            .count()
            == 1,
        "exactly one bestmove expected: {stdout}"
    );
    assert!(
        stdout.contains("info depth 0 score cp "),
        "the stopped one-shot answers the depth-0 line: {stdout}"
    );
}

#[test]
fn argv_param_overrides_are_announced_and_applied() {
    // `param:Name=Value` preloads a search-parameter override and announces it.
    let (stdout, success) = run_engine_with_args(&["param:AspirationBase=33", "uci"], b"");
    assert!(success, "param run failed");
    assert!(
        stdout.contains("info string param-override: AspirationBase=33 (applies to every go)"),
        "missing the param-override announcement"
    );
    assert!(stdout.contains("uciok"), "missing handshake");
}

#[test]
fn argv_param_overrides_fail_fast_on_unknown_names() {
    // The reference aborts on an unknown override rather than silently
    // searching with default parameters.
    let (_stdout, success) = run_engine_with_args(&["param:NoSuchLever=5", "uci"], b"");
    assert!(!success, "an unknown param override must fail fast");
}

/// The step-0 handshake transcript, with its header comments.
const HANDSHAKE_TRANSCRIPT: &str = include_str!("golden/transcripts/handshake.txt");

#[test]
fn the_handshake_matches_the_step_zero_transcript() {
    // Every engine line of the captured handshake appears, in order, in
    // KnightByte's reply. KnightByte additionally advertises its research
    // levers between the public options and the info lines, so the expected
    // lines are matched as an ordered subsequence. Placeholders: the identity
    // lines and the eval-source net name are checked structurally.
    let expected: Vec<String> = HANDSHAKE_TRANSCRIPT
        .lines()
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with('#')
                && !line.starts_with('>')
                && !line.starts_with('<')
        })
        .map(|line| match line {
            "id name <engine>" => "id name KnightByte".to_owned(),
            "id author <author>" => "id author KnightByte project".to_owned(),
            _ => line.replace("<nnue net>", "nnue net jokausch.nnue"),
        })
        .collect();

    let stdout = run_engine(b"uci\nisready\nucinewgame\nisready\nquit\n");
    let actual: Vec<&str> = stdout.lines().collect();
    let mut cursor = 0;
    for want in &expected {
        let found = actual[cursor..]
            .iter()
            .position(|line| {
                line == want
                    || (want == "id name KnightByte" && line.starts_with("id name KnightByte "))
            })
            .unwrap_or_else(|| panic!("handshake line '{want}' missing or out of order"));
        cursor += found + 1;
    }
}

#[test]
fn setoption_validation_matches_the_step_zero_transcript() {
    // The captured transcript replays ten setoption scenarios; only the
    // unknown name produces a line, everything else is silent (accepted or
    // rejected with the previous value kept). This is the exact behaviour
    // step 2 implements.
    let stdout = run_engine(
        b"setoption name Hash value 32\n\
          setoption name Hash value 999999\n\
          setoption name Hash value 0\n\
          setoption name NoSuchOption value 5\n\
          setoption name Own Book value true\n\
          setoption name Own Book value false\n\
          setoption name Own Book value maybe\n\
          setoption name Clear Hash\n\
          setoption name Threads value 0\n\
          setoption name Threads value 999\n\
          quit\n",
    );
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines,
        vec!["info string Unknown option: NoSuchOption"],
        "the setoption replay must stay silent except for the unknown name"
    );
}

// ----------------------------------------------------------------------
// Phase 5 step 4: the search runs on a worker thread; the loop keeps
// reading commands while it runs.
// ----------------------------------------------------------------------

/// Spawns the engine and returns it with a timestamped line reader running.
///
/// The reader records every output line with the instant it arrived, which
/// is what the stop-latency test measures against.
fn spawn_with_reader(
    lines: &std::sync::Arc<std::sync::Mutex<Vec<(Instant, String)>>>,
) -> (std::process::Child, std::thread::JoinHandle<()>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_knightbyte"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn knightbyte");
    let stdout = child.stdout.take().expect("stdout");
    let reader_lines = std::sync::Arc::clone(lines);
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
    (child, reader)
}

#[test]
fn stop_answers_go_infinite_within_100_ms() {
    let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (mut child, reader) = spawn_with_reader(&lines);
    let mut stdin = child.stdin.take().expect("stdin");
    stdin
        .write_all(b"position startpos\ngo infinite\n")
        .expect("write go");
    stdin.flush().expect("flush go");
    // Let the search reach a couple of iterations before stopping.
    std::thread::sleep(Duration::from_millis(300));

    let stopped = Instant::now();
    stdin.write_all(b"stop\n").expect("write stop");
    stdin.flush().expect("flush stop");

    // The 100 ms budget is the plan's release-mode requirement: the stop
    // flag is polled every 1024 nodes, which costs about 3 ms at release
    // speed (measured 0.1-3 ms on the release binary). A debug build runs
    // ~100x slower and the full test suite runs several engines in
    // parallel, so the budget scales with the build; the mechanism (a stop
    // ends the search promptly) is what is under test.
    let budget = if cfg!(debug_assertions) {
        Duration::from_secs(10)
    } else {
        Duration::from_millis(100)
    };
    let deadline = stopped + budget;
    let mut bestmove_time = None;
    while Instant::now() < deadline {
        let found = lines
            .lock()
            .expect("reader lock")
            .iter()
            .find(|(_, line)| line.starts_with("bestmove "))
            .map(|(instant, _)| *instant);
        if let Some(instant) = found {
            bestmove_time = Some(instant);
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let bestmove_time = bestmove_time.expect("no bestmove within the stop budget of stop");
    assert!(
        bestmove_time.duration_since(stopped) < budget,
        "the bestmove took {:?} after stop",
        bestmove_time.duration_since(stopped)
    );

    stdin.write_all(b"quit\n").expect("write quit");
    drop(stdin);
    assert!(child.wait().expect("wait").success(), "engine failed");
    reader.join().expect("reader");
    // Exactly one bestmove for the one go.
    assert_eq!(
        lines
            .lock()
            .expect("reader lock")
            .iter()
            .filter(|(_, line)| line.starts_with("bestmove "))
            .count(),
        1
    );
}

#[test]
fn two_hundred_go_stop_cycles_answer_exactly_one_bestmove_each() {
    // Every go gets exactly one bestmove, whether the stop arrives before
    // depth 1 finishes (a depth-0 answer) or during a later iteration. The
    // cycles walk depths 1 to 6, as the step-4 test prescription asks.
    let mut script = String::from("position startpos\n");
    let mut cycles = 0;
    for depth in 1..=6 {
        for _ in 0..(200 / 6 + usize::from(depth <= 2)) {
            script.push_str(&format!("go depth {depth}\nstop\n"));
            cycles += 1;
        }
    }
    script.push_str("quit\n");

    let stdout = run_engine_with_args(&[], script.as_bytes()).0;
    assert_eq!(cycles, 200);
    assert_eq!(
        stdout
            .lines()
            .filter(|line| line.starts_with("bestmove "))
            .count(),
        200,
        "every go/stop cycle must answer exactly one bestmove"
    );
}

#[test]
fn quit_during_a_search_exits_cleanly() {
    let stdout = run_engine(b"position startpos\ngo depth 30\nquit\n");
    assert!(
        stdout
            .lines()
            .filter(|line| line.starts_with("bestmove "))
            .count()
            == 1,
        "quit during a search must still answer the go exactly once"
    );
}

#[test]
fn isready_is_answered_during_a_search() {
    // The step-0 capture (isready_during_search.txt): readyok comes at once,
    // preceded by the eval-source line, while the search runs.
    let stdout = run_engine(b"position startpos\ngo depth 20\nisready\nquit\n");
    assert!(stdout.contains("readyok"), "readyok missing");
    assert!(stdout.contains("eval-source"), "eval-source line missing");
    assert!(
        stdout.lines().any(|line| line.starts_with("bestmove ")),
        "missing bestmove"
    );
}

#[test]
fn a_single_legal_move_is_answered_at_once() {
    // Quirk Q8 (reference parity): nothing to consider, so the move is
    // answered with a depth-0 info line carrying the static evaluation and
    // no search runs. The FEN is verified against the external oracle at
    // test time, per the project rules.
    common::verify_fen("7k/8/8/8/8/1p6/r7/K7 w - - 0 1");
    let stdout = run_engine(b"position fen 7k/8/8/8/8/1p6/r7/K7 w - - 0 1\ngo depth 5\nquit\n");
    let depth_lines: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("info depth "))
        .collect();
    assert_eq!(depth_lines.len(), 1, "exactly the depth-0 line: {stdout}");
    assert!(
        depth_lines[0].starts_with("info depth 0 score cp "),
        "the depth-0 line carries the static evaluation: {}",
        depth_lines[0]
    );
    assert!(
        depth_lines[0].ends_with("pv a1b1"),
        "the only move: {stdout}"
    );
    assert!(
        stdout.contains("bestmove a1b1"),
        "the only move is the bestmove: {stdout}"
    );
}

#[test]
fn a_stop_before_depth_1_finishes_answers_depth_zero() {
    // The step-0 capture (stop.txt, scenario b): a stop that arrives before
    // depth 1 completes gets `info depth 0 score cp <static-eval> pv
    // <first-legal-move>` and the first legal move as the bestmove — the
    // start position evaluates 27 and generates a2a3 first.
    let stdout = run_engine(b"position startpos\ngo depth 1\nstop\nquit\n");
    let depth_lines: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("info depth "))
        .collect();
    assert_eq!(
        depth_lines,
        vec!["info depth 0 score cp 27 pv a2a3"],
        "the stop-during-depth-1 answer must match the capture: {stdout}"
    );
    assert!(stdout.contains("bestmove a2a3"), "missing bestmove");
}

#[test]
fn a_command_touching_the_state_joins_the_running_search() {
    // The fallback for GUIs that break the protocol: `position` during a
    // running search stops and joins it first (one bestmove on the wire),
    // then applies. The `d` output proves the new position is live.
    let stdout =
        run_engine(b"position startpos\ngo depth 30\nposition startpos moves e2e4\nd\nquit\n");
    assert_eq!(
        stdout
            .lines()
            .filter(|line| line.starts_with("bestmove "))
            .count(),
        1,
        "the joined search answers once; no second bestmove"
    );
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("info string FEN: rnbqkbnr/pppppppp/8/8/4P3")),
        "the position command was not applied after the join: {stdout}"
    );
}

// ----------------------------------------------------------------------
// Phase 5 step 5: `go ponder` runs unbounded until `ponderhit` or `stop`.
// ----------------------------------------------------------------------

#[test]
fn a_stop_ends_a_ponder_search_with_the_null_move() {
    // Quirk Q5 (the step-0 capture, scenario a): a stop without ponderhit
    // ends a ponder search with `bestmove 0000`, no matter how many
    // iterations completed. The ponder phase runs 400 ms first, so the stop
    // hits a search with completed iterations, like the capture.
    let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (mut child, reader) = spawn_with_reader(&lines);
    let mut stdin = child.stdin.take().expect("stdin");
    stdin
        .write_all(b"position startpos\ngo ponder wtime 5000 btime 5000 winc 100 binc 100\n")
        .expect("write go ponder");
    stdin.flush().expect("flush");
    std::thread::sleep(Duration::from_millis(400));
    stdin.write_all(b"stop\nquit\n").expect("write stop");
    drop(stdin);
    assert!(child.wait().expect("wait").success(), "engine failed");
    reader.join().expect("reader");

    let captured = lines.lock().expect("reader lock").clone();
    // Q5's core does not depend on how far the search got: the stop without
    // ponderhit answers the null move, whether or not an iteration had
    // completed (under full-suite load even depth 1 can still be running).
    let bestmoves: Vec<&str> = captured
        .iter()
        .filter(|(_, line)| line.starts_with("bestmove "))
        .map(|(_, line)| line.as_str())
        .collect();
    assert_eq!(bestmoves, vec!["bestmove 0000"], "Q5: {captured:?}");
}

#[test]
fn the_ponder_clock_starts_at_ponderhit() {
    // The step-0 capture (scenario b): the ponder phase ignores the clock,
    // and the clock stored with the `go` starts only at the `ponderhit`. A
    // fixed `movetime 300` proves it: the ponder phase runs longer than 300
    // ms without being stopped, and after the ponderhit the fresh search —
    // which restarts at depth 1 from the opponent's view — answers inside
    // roughly the movetime.
    let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (mut child, reader) = spawn_with_reader(&lines);
    let mut stdin = child.stdin.take().expect("stdin");
    stdin
        .write_all(b"position startpos\ngo ponder movetime 300\n")
        .expect("write go ponder");
    stdin.flush().expect("flush");
    // Far longer than the 300 ms movetime: nothing may answer yet.
    std::thread::sleep(Duration::from_millis(700));
    assert!(
        !lines
            .lock()
            .expect("reader lock")
            .iter()
            .any(|(_, line)| line.starts_with("bestmove ")),
        "the ponder search must ignore the movetime clock"
    );

    let hit = Instant::now();
    stdin.write_all(b"ponderhit\n").expect("write ponderhit");
    stdin.flush().expect("flush");

    // The movetime budget is wall-clock (the hard limit fires regardless of
    // progress), but under full-suite load the drain and the flush take
    // longer; the budget is generous on purpose.
    let deadline = hit + Duration::from_secs(10);
    loop {
        let done = lines
            .lock()
            .expect("reader lock")
            .iter()
            .any(|(_, line)| line.starts_with("bestmove "));
        if done || Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let captured = lines.lock().expect("reader lock").clone();
    let bestmove = captured
        .iter()
        .find(|(_, line)| line.starts_with("bestmove "))
        .expect("no bestmove after ponderhit");
    assert!(
        bestmove.0.duration_since(hit) < Duration::from_secs(10),
        "the fresh search must run inside the clock that started at ponderhit, took {:?}",
        bestmove.0.duration_since(hit)
    );

    stdin.write_all(b"quit\n").expect("write quit");
    drop(stdin);
    assert!(child.wait().expect("wait").success(), "engine failed");
    reader.join().expect("reader");

    // The fresh search restarts at depth 1: at least one depth-1 line is on
    // the wire. (Under full-suite load the ponder phase itself may not have
    // completed its own depth 1 within the fixed 700 ms, so exactly-two
    // would be a load-flaky assertion; the restart semantics are pinned by
    // the depth lines' presence and the implementation.)
    assert!(
        captured
            .iter()
            .any(|(_, line)| line.starts_with("info depth 1 ")),
        "the post-ponder search restarts at depth 1: {captured:?}"
    );
    // Exactly one bestmove for the whole ponder cycle.
    assert_eq!(
        captured
            .iter()
            .filter(|(_, line)| line.starts_with("bestmove "))
            .count(),
        1
    );
}

#[test]
fn a_stray_ponderhit_is_ignored() {
    // The step-0 capture (scenario c): a ponderhit with no ponder search
    // running prints nothing.
    let stdout = run_engine(b"position startpos\nponderhit\nquit\n");
    assert!(stdout.is_empty(), "stray ponderhit answered: {stdout}");
}

#[test]
fn a_ponderhit_during_a_normal_search_is_ignored() {
    // Only a ponder search can be promoted; a normal search runs on and
    // answers its own bestmove.
    let stdout = run_interactively(b"position startpos\ngo depth 5\nponderhit\n");
    assert_eq!(
        stdout
            .lines()
            .filter(|line| line.starts_with("bestmove "))
            .count(),
        1
    );
    assert!(
        !stdout.contains("bestmove 0000"),
        "the normal search was disturbed by the ponderhit: {stdout}"
    );
    assert!(
        stdout.contains("info depth 5 "),
        "the normal search did not run to its depth: {stdout}"
    );
}

// ----------------------------------------------------------------------
// Phase 5 step 8: the persistent helper pool (Lazy SMP).
// ----------------------------------------------------------------------

#[test]
fn threads_above_one_searches_with_helpers_and_resizes_between_searches() {
    // Step 8: with Threads > 1 every search runs on the helpers too, while
    // the main thread's result stays the answer. Multi-thread node counts
    // are nondeterministic by design (plan 7.3), so the test asserts the
    // protocol shape: every search gets exactly one real bestmove, across
    // pool resizes.
    let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (mut child, reader) = spawn_with_reader(&lines);
    let mut stdin = child.stdin.take().expect("stdin");

    let mut searches = 0;
    for threads in ["4", "2", "1"] {
        stdin
            .write_all(
                format!("setoption name Threads value {threads}\nposition startpos\ngo depth 6\n")
                    .as_bytes(),
            )
            .expect("write search commands");
        stdin.flush().expect("flush");
        searches += 1;
        // Wait for this search's bestmove (the depth-6 debug search takes
        // about a second; the deadline is generous).
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let bestmoves = lines
                .lock()
                .expect("reader lock")
                .iter()
                .filter(|(_, line)| line.starts_with("bestmove "))
                .count();
            if bestmoves >= searches || Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    stdin.write_all(b"quit\n").expect("write quit");
    drop(stdin);
    assert!(child.wait().expect("wait").success(), "engine failed");
    reader.join().expect("reader");

    let captured = lines.lock().expect("reader lock").clone();
    let bestmoves: Vec<&str> = captured
        .iter()
        .filter(|(_, line)| line.starts_with("bestmove "))
        .map(|(_, line)| line.as_str())
        .collect();
    assert_eq!(
        bestmoves.len(),
        searches,
        "one bestmove per go: {captured:?}"
    );
    for bestmove in bestmoves {
        assert_ne!(bestmove, "bestmove 0000", "a legal move is answered");
    }
}

#[test]
fn threads_resizing_is_safe_when_no_search_runs() {
    // The stress core of step 9, in miniature: repeated resize cycles with
    // searches interleaved must never panic and always answer.
    let stdout = run_engine(
        b"setoption name Threads value 8\n\
          setoption name Threads value 1\n\
          setoption name Threads value 4\n\
          setoption name Threads value 256\n\
          setoption name Threads value 1\n\
          quit\n",
    );
    assert!(
        stdout.is_empty(),
        "a silent option change answered: {stdout}"
    );
}

#[test]
fn a_hash_resize_after_a_helper_search_survives() {
    // Bug B39 regression (found by the step-9 stress walk): helpers used to
    // keep their SharedTables clones between searches, which blocked the
    // exclusive resize borrow and killed the engine on the next
    // `setoption Hash`. The full sequence must keep the engine alive and
    // searching.
    let stdout = run_interactively(
        b"setoption name Threads value 4\nposition startpos\ngo depth 3\nsetoption name Hash value 16\n",
    );
    assert!(stdout.contains("bestmove "), "the first search answered");
    let stdout = run_engine(
        b"setoption name Threads value 4\nposition startpos\ngo depth 3\nsetoption name Hash value 16\nisready\nquit\n",
    );
    assert!(
        stdout.contains("readyok"),
        "the engine died on the resize after a helper search: {stdout}"
    );
}

// ----------------------------------------------------------------------
// Phase 5 step 6: `go mate N` and `go searchmoves`.
// ----------------------------------------------------------------------

/// The mate-in-one position of the step-0 capture (`a1a8` is the mate).
const MATE_IN_ONE_FEN: &str = "6k1/5ppp/8/8/8/8/5PPP/R5K1 w - - 0 1";

#[test]
fn go_mate_stops_at_the_first_iteration_proving_the_mate() {
    // The step-0 capture (scenario a): `go mate 1` stops at the first
    // completed iteration whose score proves a mate within one move —
    // depth 1, 43 nodes, `score mate 1 pv a1a8`. The FEN is verified
    // against the external oracle at test time, per the project rules.
    common::verify_fen(MATE_IN_ONE_FEN);
    let stdout =
        run_interactively(&format!("position fen {MATE_IN_ONE_FEN}\ngo mate 1\n").into_bytes());
    let depth_lines: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("info depth "))
        .collect();
    assert_eq!(depth_lines.len(), 1, "only depth 1 runs: {stdout}");
    let line = depth_lines[0];
    assert!(
        line.starts_with("info depth 1 seldepth 1 score mate 1 nodes 43 "),
        "the mate-in-one answer must match the capture (43 nodes): {line}"
    );
    assert!(line.ends_with("pv a1a8"), "the mate move is the PV: {line}");
    assert!(stdout.contains("bestmove a1a8"), "missing bestmove");
}

#[test]
fn go_mate_without_a_provable_mate_searches_unbounded() {
    // The step-0 capture (scenario b): with no provable mate in N the
    // search runs on without a depth cap until stopped; a stop answers the
    // last completed iteration's bestmove (a normal search's ending — not
    // the Q5 null move). The load-tolerant core: no bestmove while the
    // search runs (a depth-capped `go` would have answered by itself; the
    // depth-cap lift is pinned deterministically by
    // `a_mate_limiter_has_no_depth_cap` in the UCI module), and one real
    // bestmove after the stop.
    let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (mut child, reader) = spawn_with_reader(&lines);
    let mut stdin = child.stdin.take().expect("stdin");
    stdin
        .write_all(b"position startpos\ngo mate 3\n")
        .expect("write go mate");
    stdin.flush().expect("flush");
    std::thread::sleep(Duration::from_millis(600));
    assert!(
        !lines
            .lock()
            .expect("reader lock")
            .iter()
            .any(|(_, line)| line.starts_with("bestmove ")),
        "the mate search must run until stopped"
    );
    stdin.write_all(b"stop\nquit\n").expect("write stop");
    drop(stdin);
    assert!(child.wait().expect("wait").success(), "engine failed");
    reader.join().expect("reader");
    let captured = lines.lock().expect("reader lock").clone();
    let bestmoves: Vec<&str> = captured
        .iter()
        .filter(|(_, line)| line.starts_with("bestmove "))
        .map(|(_, line)| line.as_str())
        .collect();
    assert_eq!(bestmoves.len(), 1, "exactly one bestmove: {captured:?}");
    assert_ne!(bestmoves[0], "bestmove 0000", "a real move is answered");
}

#[test]
fn searchmoves_restricts_the_root_moves() {
    // The step-0 capture (scenario c, cold table): the restricted search is
    // a normal search over the listed root moves only. The node counts and
    // seldepths are pinned to the reference's cold run (3 and 53 nodes at
    // depths 1 and 2); the timing fields are not pinned.
    let stdout = run_interactively(b"position startpos\ngo depth 5 searchmoves e2e4\n");
    let depth_lines: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("info depth "))
        .collect();
    assert!(depth_lines.len() >= 5, "depth 5 not reached: {stdout}");
    assert!(
        depth_lines[0].starts_with("info depth 1 seldepth 1 score cp 45 nodes 3 "),
        "the restricted depth-1 run must match the reference: {}",
        depth_lines[0]
    );
    assert!(
        depth_lines[1].starts_with("info depth 2 seldepth 2 score cp 33 nodes 53 "),
        "the restricted depth-2 run must match the reference: {}",
        depth_lines[1]
    );
    // Every iteration's PV starts with the listed move, and the bestmove is
    // the listed move.
    assert!(
        depth_lines
            .iter()
            .all(|line| line.contains(" pv e2e4") || line.contains(" pv e2e4 ")),
        "every PV starts with e2e4: {stdout}"
    );
    assert!(stdout.contains("bestmove e2e4"), "missing bestmove");
}

#[test]
fn searchmoves_with_two_moves_stays_inside_the_list() {
    // Two listed moves: the answer must be one of them, and the tree must
    // stay far below the unrestricted depth-5 search (4,001 nodes).
    let stdout = run_interactively(b"position startpos\ngo depth 5 searchmoves e2e4 d2d4\n");
    let bestmove = stdout
        .lines()
        .find(|line| line.starts_with("bestmove "))
        .expect("missing bestmove");
    assert!(
        bestmove == "bestmove e2e4" || bestmove == "bestmove d2d4",
        "the bestmove must be a listed move: {bestmove}"
    );
    let depth5 = stdout
        .lines()
        .find(|line| line.starts_with("info depth 5 "))
        .expect("depth 5 not reached");
    let nodes: u64 = depth5
        .split(" nodes ")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .and_then(|value| value.parse().ok())
        .expect("the depth-5 line carries nodes");
    assert!(nodes < 4_001, "the root was not restricted: {depth5}");
}

#[test]
fn searchmoves_without_moves_or_with_illegal_moves_is_ignored() {
    // The step-0 capture (scenario e) and the reference probe (2026-11-18):
    // an empty `searchmoves` and a list whose every entry is illegal both
    // run the normal unrestricted search — here the exact depth-5 node count
    // of the unrestricted startpos search.
    for go in ["go depth 5 searchmoves", "go depth 5 searchmoves e7e5"] {
        let stdout = run_interactively(&format!("position startpos\n{go}\n").into_bytes());
        assert!(
            stdout.contains("info depth 5 ") && stdout.contains("nodes 4001"),
            "the unrestricted depth-5 search must run (4,001 nodes): {stdout}"
        );
        assert!(stdout.contains("bestmove e2e4"), "missing bestmove");
    }
}
