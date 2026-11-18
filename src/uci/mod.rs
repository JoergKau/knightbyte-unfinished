//! UCI command loop.
//!
//! The loop owns the board and the search state. It handles the handshake, the
//! diagnostic commands (`position`, `perft`, `nnueeval`, `bench`) and a minimal
//! `go` that honours the `depth` and `nodes` limiters. Clock limiters, `stop`
//! and threading arrive with the UCI phase; an unsupported limiter is reported
//! instead of silently searching forever.

use std::io::{self, BufRead};

use crate::board::fen::STARTPOS_FEN;
use crate::board::movegen::legal;
use crate::board::perft::perft;
use crate::board::position::Position;
use crate::eval::nnue::evaluate_position;
use crate::output::Output;
use crate::search::limits::Limits;
use crate::search::options::{LEVERS, OptionKind, PUBLIC_OPTIONS};
use crate::search::searcher::{MATE_LIMIT, MATE_SCORE, SearchInfo, Searcher};
use crate::search::time_manager::TimeControl;
use crate::tools::bench;

pub mod parser;

/// Depth used when `go` names no limiter.
const DEFAULT_GO_DEPTH: u32 = 8;

/// Transposition-table size for the command loop, in megabytes.
const DEFAULT_TT_MEGABYTES: usize = 256;

/// Runs the UCI event loop until `quit` or end of input.
///
/// # Errors
///
/// Returns an I/O error if reading standard input or writing standard output
/// fails. A disconnected GUI is treated as end of input, which lets the
/// process exit cleanly.
pub fn run() -> io::Result<()> {
    let stdin = io::stdin();
    let mut output = Output::new();
    // Reference parity (the predecessor's bug-143 fix): the engine starts on
    // the start position, so a `go` before any `position` command searches
    // something sensible instead of answering a null bestmove. The option
    // stays in the type because the answer path below remains as robustness.
    let mut position: Option<Position> = Some(start_position());
    let mut searcher = Searcher::new(DEFAULT_TT_MEGABYTES);

    for line in stdin.lock().lines() {
        let line = line?;
        let command = line.trim();
        if command.is_empty() {
            continue;
        }

        match parser::parse(command) {
            parser::Command::Uci => handshake(&mut output)?,
            parser::Command::IsReady => output.line("readyok")?,
            // Reference parity (the predecessor's bug-143 fix): a new game
            // resets to the start position, so `go` after `ucinewgame` never
            // searches the previous game's position. The game history is
            // reset with it: the start position has no earlier positions.
            parser::Command::UciNewGame => {
                searcher.clear();
                position = Some(start_position());
                searcher.set_game_history(&[]);
            }
            parser::Command::Bench => bench::run(command, &mut output)?,
            parser::Command::SetOption => handle_set_option(command, &mut searcher, &mut output)?,
            parser::Command::Position => match parse_position(command) {
                Ok((next, earlier_keys)) => {
                    searcher.set_game_history(&earlier_keys);
                    position = Some(next);
                }
                // Reference parity (the predecessor's bug-144/145 decision): a
                // rejected `position` command is reported loudly and keeps the
                // previous position, so the engine never searches a position
                // the GUI did not ask for.
                Err(error) => output.line(&format!("info string {}", error.message()))?,
            },
            parser::Command::Go => {
                handle_go(command, position.as_ref(), &mut searcher, &mut output)?
            }
            parser::Command::Perft => handle_perft(command, position.as_ref(), &mut output)?,
            parser::Command::NnueEval => handle_nnue_eval(position.as_ref(), &mut output)?,
            parser::Command::Quit => break,
            parser::Command::Unknown => {
                output.line(&format!("info string unknown command: {command}"))?;
            }
        }
    }

    Ok(())
}

/// Why a `position` command was rejected.
///
/// The message texts mirror the fixed reference exactly (the predecessor's
/// bug-144/145 decision, commit bffe98a): a rejected command is reported
/// loudly and the previous position stays, so the engine never silently
/// searches a position the GUI did not ask for.
enum PositionError {
    /// The FEN or the command shape is unusable.
    InvalidPosition(String),
    /// A move in the `moves` tail is not legal.
    IllegalMove(String),
}

impl PositionError {
    /// Returns the `info string` text the engine reports for this rejection.
    fn message(&self) -> String {
        match self {
            Self::InvalidPosition(detail) => {
                format!("Error: invalid position, previous position kept: {detail}")
            }
            Self::IllegalMove(mv) => format!(
                "Error: illegal move {mv}, position command rejected, previous position kept"
            ),
        }
    }
}

/// Parses `position startpos|fen <fields>` with an optional `moves` tail.
///
/// Returns the final position and the keys of every position that preceded it
/// in the game (the starting position and each intermediate one, oldest
/// first), which the search needs to recognise repetitions of earlier game
/// positions.
fn parse_position(line: &str) -> Result<(Position, Vec<u64>), PositionError> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let (mut position, next) = match tokens.get(1).copied() {
        Some("startpos") => (start_position(), 2),
        Some("fen") if tokens.len() >= 8 => {
            let fen = tokens[2..8].join(" ");
            (
                Position::from_fen(&fen)
                    .map_err(|error| PositionError::InvalidPosition(error.to_string()))?,
                8,
            )
        }
        _ => {
            return Err(PositionError::InvalidPosition(
                "command expects 'startpos' or 'fen <six fields>'".to_owned(),
            ));
        }
    };

    let mut earlier_keys = Vec::new();
    match tokens.get(next).copied() {
        Some("moves") => {
            for text in &tokens[next + 1..] {
                earlier_keys.push(position.key());
                position = apply_uci_move(&position, text)?;
            }
        }
        Some(unexpected) => {
            return Err(PositionError::InvalidPosition(format!(
                "unexpected token '{unexpected}' in position command"
            )));
        }
        None => {}
    }
    Ok((position, earlier_keys))
}

/// Returns a fresh start position.
///
/// A `position startpos` command and the `ucinewgame` reset both build the
/// position from the built-in constant, which always parses.
fn start_position() -> Position {
    Position::from_fen(STARTPOS_FEN).expect("the built-in start position must parse")
}

/// Applies one UCI move string to `position`.
fn apply_uci_move(position: &Position, text: &str) -> Result<Position, PositionError> {
    let moves = legal(position);
    for &mv in moves.as_slice() {
        if mv.to_uci() == text {
            return Ok(position.make_move(mv));
        }
    }
    Err(PositionError::IllegalMove(text.to_owned()))
}

/// Parses the limiters of a `go` command.
///
/// Clock limiters are read as absolute white/black values and then selected for
/// the side to move. Returns the search limits (depth/nodes) and the
/// time-manager setup.
fn parse_go(
    line: &str,
    position: Option<&Position>,
) -> Result<(Limits, Option<TimeSetup>), String> {
    let mut limits = Limits::default();
    let mut limited = false;
    let mut fixed_time = None;
    let mut white_time = 0_u64;
    let mut black_time = 0_u64;
    let mut white_inc = 0_u64;
    let mut black_inc = 0_u64;
    let mut moves_to_go = None;
    let mut clock_seen = false;
    let time_setup: Option<TimeSetup>;
    let mut tokens = line.split_whitespace().skip(1);

    while let Some(token) = tokens.next() {
        match token {
            "depth" => {
                let value = tokens
                    .next()
                    .and_then(|value| value.parse::<u32>().ok())
                    .ok_or_else(|| "go depth needs a number".to_owned())?;
                limits.depth = Some(value);
                limited = true;
            }
            "nodes" => {
                limits.nodes = Some(
                    tokens
                        .next()
                        .and_then(|value| value.parse::<u64>().ok())
                        .ok_or_else(|| "go nodes needs a number".to_owned())?,
                );
                limited = true;
            }
            "movetime" => {
                fixed_time = Some(
                    tokens
                        .next()
                        .and_then(|value| value.parse::<u64>().ok())
                        .ok_or_else(|| "go movetime needs a number".to_owned())?,
                );
                limited = true;
            }
            "wtime" | "btime" | "winc" | "binc" | "movestogo" => {
                let value = tokens
                    .next()
                    .and_then(|value| value.parse::<u64>().ok())
                    .ok_or_else(|| format!("go {token} needs a number"))?;
                clock_seen = true;
                limited = true;
                match token {
                    "wtime" => white_time = value,
                    "btime" => black_time = value,
                    "winc" => white_inc = value,
                    "binc" => black_inc = value,
                    _ => moves_to_go = Some(value as u32),
                }
            }
            "infinite" => {
                limited = true;
            }
            other => {
                return Err(format!("go limiter '{other}' is not supported yet"));
            }
        }
    }

    if let Some(move_time) = fixed_time {
        time_setup = Some(TimeSetup::Fixed(move_time));
    } else if clock_seen {
        let white_to_move = position.is_none_or(|position| position.side_to_move_is_white());
        time_setup = Some(TimeSetup::Clock(TimeControl {
            time_left_ms: if white_to_move {
                white_time
            } else {
                black_time
            },
            opponent_time_ms: if white_to_move {
                black_time
            } else {
                white_time
            },
            increment_ms: if white_to_move { white_inc } else { black_inc },
            moves_to_go,
        }));
    } else {
        time_setup = Some(TimeSetup::Infinite);
    }

    if !limited {
        limits.depth = Some(DEFAULT_GO_DEPTH);
    }
    Ok((limits, time_setup))
}

/// The time-manager setup extracted from a `go` command.
#[derive(Debug)]
enum TimeSetup {
    /// A fixed `movetime` budget.
    Fixed(u64),
    /// Clock-controlled search.
    Clock(TimeControl),
    /// No time limit.
    Infinite,
}

/// Searches the current position and prints the UCI transcript.
fn handle_go(
    line: &str,
    position: Option<&Position>,
    searcher: &mut Searcher,
    output: &mut Output,
) -> io::Result<()> {
    let (limits, time_setup) = match parse_go(line, position) {
        Ok((limits, time_setup)) => (limits, time_setup),
        Err(message) => {
            output.line(&format!("info string {message}"))?;
            return answer_without_search(position, output);
        }
    };
    let Some(position) = position else {
        output.line("info string no position set")?;
        return answer_without_search(None, output);
    };
    match time_setup {
        Some(TimeSetup::Fixed(move_time)) => searcher.init_movetime(move_time),
        Some(TimeSetup::Clock(control)) => searcher.init_clock(&control),
        _ => searcher.init_infinite(),
    }

    let mut write_error: Option<io::Error> = None;
    let result = searcher.search(position, limits, &mut |info| {
        if write_error.is_some() {
            return;
        }
        let line = format_info(info);
        if let Err(error) = output.line(&line) {
            write_error = Some(error);
        }
    });
    if let Some(error) = write_error {
        return Err(error);
    }

    output.line(&format!("bestmove {}", result.best_move.to_uci()))
}

/// Answers a `go` that cannot be searched (an unsupported limiter, a missing
/// position) with a move anyway.
///
/// A GUI waits for a `bestmove` after every `go`; staying silent would leave it
/// waiting until its clock runs out, which is a lost game. The first legal move
/// is a legal, if unconsidered, answer; with no legal move or no position the
/// null move `0000` is the protocol's way to say so.
fn answer_without_search(position: Option<&Position>, output: &mut Output) -> io::Result<()> {
    let fallback = position
        .and_then(|position| legal(position).as_slice().first().copied())
        .map_or_else(|| "0000".to_owned(), |mv| mv.to_uci());
    output.line(&format!("bestmove {fallback}"))
}

/// Formats one `info` line for a completed iteration.
fn format_info(info: &SearchInfo) -> String {
    let pv = info
        .pv
        .iter()
        .map(|mv| mv.to_uci())
        .collect::<Vec<String>>()
        .join(" ");
    format!(
        "info depth {} seldepth {} score {} nodes {} pv {}",
        info.depth,
        info.seldepth,
        format_score(info.score),
        info.nodes,
        pv
    )
}

/// Formats a score as `cp` or as a mate distance in moves.
fn format_score(score: i32) -> String {
    if score.abs() >= MATE_LIMIT {
        let plies = MATE_SCORE - score.abs();
        let moves = (plies + 1) / 2;
        if score > 0 {
            format!("mate {moves}")
        } else {
            format!("mate -{moves}")
        }
    } else {
        format!("cp {score}")
    }
}

/// Handles the diagnostic `perft <depth>` command.
fn handle_perft(line: &str, position: Option<&Position>, output: &mut Output) -> io::Result<()> {
    let depth = line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(1);
    match position {
        Some(position) => output.line(&format!("Nodes: {}", perft(position, depth))),
        None => output.line("info string no position set"),
    }
}

/// Handles the diagnostic `nnueeval` command.
fn handle_nnue_eval(position: Option<&Position>, output: &mut Output) -> io::Result<()> {
    match position {
        Some(position) => output.line(&format!("nnueeval {}", evaluate_position(position))),
        None => output.line("nnueeval none"),
    }
}

/// Sends the UCI identity block, the option catalogue and `uciok`.
fn handshake(output: &mut Output) -> io::Result<()> {
    output.line(&format!("id name KnightByte {}", env!("CARGO_PKG_VERSION")))?;
    output.line("id author KnightByte project")?;
    for option in PUBLIC_OPTIONS {
        let mut line = format!(
            "option name {} type {}",
            option.name,
            kind_name(option.kind)
        );
        if !option.default.is_empty() {
            line.push_str(&format!(" default {}", option.default));
        }
        if let (Some(min), Some(max)) = (option.min, option.max) {
            line.push_str(&format!(" min {min} max {max}"));
        }
        output.line(&line)?;
    }
    for lever in LEVERS {
        let default = if lever.kind == OptionKind::Check {
            if lever.default != 0 { "true" } else { "false" }.to_owned()
        } else {
            lever.default.to_string()
        };
        let mut line = format!(
            "option name {} type {} default {}",
            lever.name,
            kind_name(lever.kind),
            default
        );
        if lever.kind == OptionKind::Spin {
            line.push_str(&format!(" min {} max {}", lever.min, lever.max));
        }
        output.line(&line)?;
    }
    output.line("uciok")
}

/// Returns the UCI type word for an option kind.
fn kind_name(kind: OptionKind) -> &'static str {
    match kind {
        OptionKind::Spin => "spin",
        OptionKind::Check => "check",
        OptionKind::String => "string",
        OptionKind::Button => "button",
    }
}

/// Parses and applies `setoption name <name> [value <value>]`.
fn handle_set_option(line: &str, searcher: &mut Searcher, output: &mut Output) -> io::Result<()> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.get(1) != Some(&"name") {
        output.line("info string setoption expects 'name <name>'")?;
        return Ok(());
    }
    let value_index = tokens.iter().position(|token| *token == "value");
    let (name, value) = match value_index {
        Some(index) => (tokens[2..index].join(" "), tokens[index + 1..].join(" ")),
        None => (tokens[2..].join(" "), String::new()),
    };
    if name.is_empty() {
        output.line("info string setoption needs a name")?;
        return Ok(());
    }

    match name.as_str() {
        "Hash" => {
            if let Ok(megabytes) = value.parse::<usize>() {
                searcher.set_hash(megabytes);
            }
        }
        "Clear Hash" => searcher.clear(),
        "Move Overhead" => {
            if let Ok(milliseconds) = value.parse::<u64>() {
                searcher.set_move_overhead(milliseconds);
            }
        }
        "Threads" => {
            // Threading arrives with Phase 5; accept the option so scripts keep
            // working and the engine keeps searching on one thread.
        }
        other => {
            if let Some(spec) = crate::search::options::lever(other) {
                let parsed = match spec.kind {
                    OptionKind::Check => i64::from(value == "true"),
                    _ => value.parse::<i64>().unwrap_or(spec.default),
                };
                crate::search::options::set(searcher.params_mut(), spec.name, parsed);
            }
            // Unknown public names (book, tablebase, EvalFile, Contempt) are
            // accepted silently: their phases are not implemented yet.
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{TimeSetup, parse_go};
    use crate::board::fen::STARTPOS_FEN;
    use crate::board::position::Position;

    fn startpos() -> Position {
        Position::from_fen(STARTPOS_FEN).unwrap()
    }

    #[test]
    fn depth_and_nodes_limiters_parse() {
        let (by_depth, setup) = parse_go("go depth 5", None).unwrap();
        assert_eq!(by_depth.depth, Some(5));
        assert!(matches!(setup, Some(TimeSetup::Infinite)));
        let (by_nodes, _) = parse_go("go nodes 1234", None).unwrap();
        assert_eq!(by_nodes.nodes, Some(1_234));
    }

    #[test]
    fn movetime_becomes_a_fixed_budget() {
        let (_, setup) = parse_go("go movetime 100", None).unwrap();
        match setup {
            Some(TimeSetup::Fixed(milliseconds)) => assert_eq!(milliseconds, 100),
            other => panic!("expected a fixed movetime setup, got {other:?}"),
        }
    }

    #[test]
    fn clocks_are_selected_for_the_side_to_move() {
        let white = startpos();
        let (_, white_setup) =
            parse_go("go wtime 1000 btime 2000 winc 0 binc 0", Some(&white)).unwrap();
        match white_setup {
            Some(TimeSetup::Clock(control)) => {
                assert_eq!(control.time_left_ms, 1_000);
                assert_eq!(control.opponent_time_ms, 2_000);
            }
            other => panic!("expected a clock setup, got {other:?}"),
        }

        let black =
            Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b KQkq - 0 1").unwrap();
        let (_, black_setup) =
            parse_go("go wtime 1000 btime 2000 winc 0 binc 0", Some(&black)).unwrap();
        match black_setup {
            Some(TimeSetup::Clock(control)) => {
                assert_eq!(control.time_left_ms, 2_000);
                assert_eq!(control.opponent_time_ms, 1_000);
            }
            other => panic!("expected a clock setup, got {other:?}"),
        }
    }

    #[test]
    fn unsupported_limiters_are_reported() {
        assert!(parse_go("go ponder", None).is_err());
        assert!(parse_go("go nonsense", None).is_err());
    }

    #[test]
    fn infinite_is_supported() {
        let (limits, setup) = parse_go("go infinite", None).unwrap();
        assert_eq!(limits.depth, None);
        assert!(matches!(setup, Some(TimeSetup::Infinite)));
    }
}
