//! End-to-end smoke test for the UCI lifecycle.

use std::io::Write;
use std::process::{Command, Stdio};

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
        .write_all(b"position startpos moves e2e4 e7e5\ngo depth 2\nquit\n")
        .expect("write search commands");

    let output = child.wait_with_output().expect("wait for knightbyte");
    assert!(output.status.success(), "engine exited with failure");
    let stdout = String::from_utf8_lossy(&output.stdout);
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
        .write_all(
            b"position startpos\nsetoption name Hash value 8\n\
              setoption name AspirationBase value 60\n\
              setoption name Clear Hash\ngo movetime 500\nquit\n",
        )
        .expect("write option commands");

    let output = child.wait_with_output().expect("wait for knightbyte");
    assert!(output.status.success(), "engine exited with failure");
    let stdout = String::from_utf8_lossy(&output.stdout);
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

#[test]
fn an_unsupported_go_limiter_still_gets_a_bestmove() {
    // A GUI waits for a bestmove after every go. An unparsable go must not
    // leave it waiting until its clock runs out.
    let stdout = run_engine(b"position startpos\ngo ponder wtime 1000 btime 1000\nquit\n");
    assert!(stdout.contains("not supported"), "missing diagnostic");
    let bestmove = stdout
        .lines()
        .find(|line| line.starts_with("bestmove "))
        .expect("missing bestmove after an unsupported go");
    assert_ne!(bestmove, "bestmove 0000", "a legal move exists");
}

#[test]
fn go_without_a_position_searches_the_start_position() {
    // Reference parity (the predecessor's bug-143 fix): the engine starts on
    // the start position and `ucinewgame` resets to it, so a bare `go` never
    // runs without a position — it searches the start position and answers a
    // real bestmove. The null-bestmove answer path stays as robustness for a
    // state that can no longer be reached through the protocol.
    let stdout = run_engine(b"go depth 3\nquit\n");
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
    let stdout = run_engine(
        b"ucinewgame\nposition startpos moves g1f3 g8f6 f3g1 f6g8\n\
          go wtime 2000 btime 2000 winc 20 binc 20\nquit\n",
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
    let stdout = run_engine(b"position startpos\ngo\nquit\n");
    assert!(stdout.contains("info depth 6 "), "depth 6 was not reached");
    assert!(stdout.contains("bestmove "), "missing bestmove");
}

#[test]
fn unknown_go_tokens_are_ignored_not_rejected() {
    // Reference parity: a `go` with an unknown token still searches instead of
    // reporting an unsupported limiter, so a GUI never loses its move.
    let stdout = run_engine(b"position startpos\ngo depth 2 nonsense\nquit\n");
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
    // UCI command line, run once, and the engine exits afterwards. A bare
    // `go depth 1` searches the start position (the predecessor's bug-143 fix).
    let (stdout, success) = run_engine_with_args(&["go depth 1"], b"");
    assert!(success, "one-shot run failed");
    assert!(stdout.contains("info depth 1 "), "no search happened");
    assert!(stdout.contains("bestmove "), "missing bestmove");
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
