//! End-to-end UCI tests for the opening book and tablebase options.
//!
//! The book is a synthetic fixture written to the temp directory; the
//! tablebases are the committed 3-man set. Both run against the real binary.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use knightbyte::board::position::Position;
use knightbyte::tb::book::{CtgBook, compute_signature};
use knightbyte::types::r#move::{Move, flags};

/// Parses `fen` after checking it against the external verifier.
fn verified(fen: &str) -> Position {
    common::verify_fen(fen);
    Position::from_fen(fen).unwrap_or_else(|error| panic!("cannot parse {fen}: {error}"))
}

/// Builds one book entry.
fn entry(signature: &[u8], moves: &[(u8, u8)], wins: u32, draws: u32, losses: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(signature);
    bytes.push((1 + 2 * moves.len()) as u8);
    for &(mv, annotation) in moves {
        bytes.push(mv);
        bytes.push(annotation);
    }
    let total = wins + draws + losses;
    for value in [total, losses, wins, draws] {
        bytes.extend_from_slice(&value.to_be_bytes()[1..]);
    }
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&[0; 3]);
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&[0; 3]);
    bytes.extend_from_slice(&[0; 4]);
    bytes.push(0);
    bytes.extend_from_slice(&[0; 2]);
    bytes
}

/// Writes a small book with one start move (e2e4) to `path`.
fn write_book(path: &std::path::Path) {
    let start = verified("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    let e4 = Move::new(12, 28, flags::DOUBLE_PUSH);
    let child = start.make_move(e4);
    let (start_sig, start_len) = compute_signature(&start);
    let (child_sig, child_len) = compute_signature(&child);
    let byte = CtgBook::encode_move_byte(&start, e4).expect("e2e4 has a book byte");

    let mut file = vec![0_u8; 4096];
    let start_entry = entry(&start_sig[..start_len], &[(byte, 0)], 0, 0, 0);
    let child_entry = entry(&child_sig[..child_len], &[], 10, 5, 0);
    for book_entry in [start_entry, child_entry] {
        let mut page = vec![0_u8; 4096];
        page[1] = 1;
        page[4..4 + book_entry.len()].copy_from_slice(&book_entry);
        file.extend_from_slice(&page);
    }
    std::fs::write(path, file).expect("write synthetic book");
}

/// Runs commands until a `bestmove` appears, then sends `quit`.
fn run_engine(commands: &str) -> String {
    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_knightbyte"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn knightbyte");

    let stdout = child.stdout.take().expect("stdout");
    let collected = Arc::clone(&lines);
    let reader = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            collected
                .lock()
                .expect("line lock")
                .push(line.trim_end().to_owned());
            line.clear();
        }
    });

    let mut stdin = child.stdin.take().expect("stdin");
    stdin
        .write_all(commands.as_bytes())
        .expect("write commands");
    stdin.flush().expect("flush commands");

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let done = lines
            .lock()
            .expect("line lock")
            .iter()
            .any(|line| line.starts_with("bestmove "));
        if done || Instant::now() > deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    stdin.write_all(b"quit\n").expect("write quit");
    drop(stdin);
    assert!(child.wait().expect("wait for engine").success());
    reader.join().expect("reader thread");

    lines.lock().expect("line lock").join("\n")
}

#[test]
fn a_book_move_is_played_instead_of_searching() {
    let path = std::env::temp_dir().join(format!("knightbyte-book-{}.ctg", std::process::id()));
    write_book(&path);
    let commands = format!(
        "setoption name Book File value {}\n\
         setoption name Own Book value true\n\
         position startpos\n\
         go movetime 100\n",
        path.display()
    );
    let output = run_engine(&commands);
    let _ = std::fs::remove_file(&path);

    assert!(
        output.contains("info string opening book loaded:"),
        "book load was not reported:\n{output}"
    );
    assert!(
        output.contains("info string book move: e2e4"),
        "book move line missing:\n{output}"
    );
    assert!(
        output.contains("bestmove e2e4"),
        "book bestmove missing:\n{output}"
    );
}

#[test]
fn own_book_off_still_searches() {
    let path = std::env::temp_dir().join(format!("knightbyte-book-off-{}.ctg", std::process::id()));
    write_book(&path);
    let commands = format!(
        "setoption name Book File value {}\n\
         setoption name Own Book value false\n\
         position startpos\n\
         go depth 2\n",
        path.display()
    );
    let output = run_engine(&commands);
    let _ = std::fs::remove_file(&path);

    assert!(!output.contains("info string book move"), "{output}");
    assert!(output.contains("info depth 1 "), "no search ran:\n{output}");
}

#[test]
fn book_depth_limits_the_book_zone() {
    let path =
        std::env::temp_dir().join(format!("knightbyte-book-depth-{}.ctg", std::process::id()));
    write_book(&path);
    let commands = format!(
        "setoption name Book File value {}\n\
         setoption name Own Book value true\n\
         setoption name Book Depth value 1\n\
         position startpos moves e2e4\n\
         go depth 2\n",
        path.display()
    );
    let output = run_engine(&commands);
    let _ = std::fs::remove_file(&path);

    assert!(!output.contains("info string book move"), "{output}");
    assert!(output.contains("info depth 1 "), "no search ran:\n{output}");
}

#[test]
fn a_missing_book_file_is_reported() {
    let output = run_engine(
        "setoption name Book File value C:\\definitely-not-a-book.ctg\n\
         position startpos\n\
         go depth 1\n",
    );
    assert!(
        output.contains("info string book file not loaded:"),
        "missing book was not reported:\n{output}"
    );
}

#[test]
fn syzygy_path_loads_and_probes_in_search() {
    let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/tb");
    let fen = "8/8/8/8/8/1Q6/8/K6k w - - 0 1";
    let _ = verified(fen);
    let commands = format!(
        "setoption name SyzygyPath value {directory}\n\
         position fen {fen}\n\
         go depth 4\n"
    );
    let output = run_engine(&commands);
    assert!(
        output.contains("info string syzygy tablebases: 3 WDL tables, up to 3 pieces"),
        "tablebase load was not reported:\n{output}"
    );
    assert!(output.contains("info depth 1 "), "no search ran:\n{output}");
    assert!(output.contains("bestmove "), "no bestmove:\n{output}");
    assert!(
        !output.contains("bestmove 0000"),
        "search failed:\n{output}"
    );
}

#[test]
fn syzygy_root_probe_plays_a_tablebase_move() {
    let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/tb");
    let fen = "8/8/8/8/8/1Q6/8/K6k w - - 0 1";
    let _ = verified(fen);
    let commands = format!(
        "setoption name SyzygyPath value {directory}\n\
         setoption name SyzygyRootProbe value true\n\
         position fen {fen}\n\
         go movetime 100\n"
    );
    let output = run_engine(&commands);
    assert!(
        output.contains("info string tablebase move:"),
        "root probe line missing:\n{output}"
    );
    assert!(output.contains("bestmove "), "no bestmove:\n{output}");
}

#[test]
fn syzygy_root_probe_default_off_still_searches() {
    let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/tb");
    let fen = "8/8/8/8/8/1Q6/8/K6k w - - 0 1";
    let _ = verified(fen);
    let commands = format!(
        "setoption name SyzygyPath value {directory}\n\
         position fen {fen}\n\
         go depth 4\n"
    );
    let output = run_engine(&commands);
    assert!(
        !output.contains("info string tablebase move:"),
        "the root probe must be off by default:\n{output}"
    );
    assert!(output.contains("info depth 1 "), "no search ran:\n{output}");
}

#[test]
fn a_mated_position_answers_the_null_move() {
    // Derived from a verified parent by playing the mate.
    let parent = "k7/8/2K5/8/8/8/8/1Q6 w - - 0 1";
    let _ = verified(parent);
    let commands = format!(
        "setoption name SyzygyPath value {}\n\
         position fen {parent} moves b1b7\n\
         go movetime 100\n",
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/tb")
    );
    let output = run_engine(&commands);
    assert!(
        output.contains("bestmove 0000"),
        "a mated position must answer the null move:\n{output}"
    );
}

#[test]
fn an_empty_syzygy_path_reports_none() {
    let output = run_engine(
        "setoption name SyzygyPath value <empty>\n\
         position startpos\n\
         go depth 1\n",
    );
    assert!(
        output.contains("info string syzygy tablebases: none (set SyzygyPath)"),
        "empty path was not reported:\n{output}"
    );
}

/// Returns the `tbhits` value of every `info depth` line, `None` for a line
/// without the field.
fn tbhits_per_info_line(output: &str) -> Vec<Option<u64>> {
    output
        .lines()
        .filter(|line| line.starts_with("info depth "))
        .map(|line| {
            let mut words = line.split_whitespace();
            words
                .by_ref()
                .find(|word| *word == "tbhits")
                .and_then(|_| words.next())
                .and_then(|value| value.parse().ok())
        })
        .collect()
}

#[test]
fn tbhits_joins_the_info_line_only_while_tablebases_are_in_use() {
    let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/tb");
    let fen = "8/8/8/8/8/1Q6/8/K6k w - - 0 1";
    let _ = verified(fen);

    // Without a tablebase path the line is the reference's, field for field.
    let plain = run_engine(&format!("position fen {fen}\ngo depth 4\n"));
    assert!(
        !plain.contains("tbhits"),
        "tbhits appeared without tablebases:\n{plain}"
    );

    // With tables every info line carries the field, and the search found hits.
    let with_tables = run_engine(&format!(
        "setoption name SyzygyPath value {directory}\nposition fen {fen}\ngo depth 4\n"
    ));
    let hits = tbhits_per_info_line(&with_tables);
    assert!(!hits.is_empty(), "no info line:\n{with_tables}");
    assert!(
        hits.iter().all(Option::is_some),
        "an info line lacks tbhits:\n{with_tables}"
    );
    let counts: Vec<u64> = hits.into_iter().flatten().collect();
    assert!(
        counts.windows(2).all(|pair| pair[0] <= pair[1]),
        "tbhits must not decrease: {counts:?}"
    );
    assert!(counts.last().copied().unwrap_or(0) > 0, "{counts:?}");

    // The field sits between hashfull and time.
    let line = with_tables
        .lines()
        .find(|line| line.starts_with("info depth "))
        .expect("an info line");
    let position = |key: &str| line.find(key).unwrap_or_else(|| panic!("no {key}: {line}"));
    assert!(position("hashfull") < position("tbhits") && position("tbhits") < position("time"));
}

#[test]
fn helper_threads_add_their_tablebase_hits() {
    let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/tb");
    let fen = "8/8/8/8/8/1Q6/8/K6k w - - 0 1";
    let _ = verified(fen);
    let output = run_engine(&format!(
        "setoption name SyzygyPath value {directory}\n\
         setoption name Threads value 3\n\
         position fen {fen}\n\
         go depth 8\n"
    ));
    let counts: Vec<u64> = tbhits_per_info_line(&output)
        .into_iter()
        .flatten()
        .collect();
    assert!(!counts.is_empty(), "no tbhits with 3 threads:\n{output}");
    assert!(counts.last().copied().unwrap_or(0) > 0, "{counts:?}");
}
