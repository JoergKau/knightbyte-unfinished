//! UCI command loop.
//!
//! The loop owns the board and the search state. It handles the handshake, the
//! protocol commands and the diagnostic surface (`position`, `perft`,
//! `nnueeval`, `d`, `eval`, `debug`, `checkmoves`), and parses every `go`
//! limiter the reference knows. Commands whose features arrive with later
//! steps (`stop`, `ponderhit`, `go ponder/mate/searchmoves`) are recognised and
//! answered so a GUI never waits, and their real behaviour lands with steps 4
//! to 6.

use std::io::{self, BufRead};

use crate::board::fen::STARTPOS_FEN;
use crate::board::movegen::{gives_check, legal};
use crate::board::perft::perft;
use crate::board::position::Position;
use crate::eval::nnue::evaluate_position;
use crate::output::Output;
use crate::search::lever_table::{LEVERS, LeverSpec};
use crate::search::limits::Limits;
use crate::search::options::{OptionKind, PUBLIC_OPTIONS, UciOption, lever, public, set};
use crate::search::searcher::{MATE_LIMIT, MATE_SCORE, SearchInfo, Searcher};
use crate::search::time_manager::TimeControl;
use crate::tools::bench;
use crate::tools::net::NET_SOURCE_NAME;

pub mod parser;

/// Depth used when `go` names no limiter (reference parity: a bare `go`
/// searches depth 6; the old KnightByte default of 8 was a recorded deviation).
const DEFAULT_GO_DEPTH: u32 = 6;

/// Transposition-table size for the command loop, in megabytes.
const DEFAULT_TT_MEGABYTES: usize = 256;

/// Runs the UCI event loop over standard input without arguments.
///
/// # Errors
///
/// Returns an I/O error if reading standard input or writing standard output
/// fails. A disconnected GUI is treated as end of input, which lets the
/// process exit cleanly.
pub fn run() -> io::Result<()> {
    run_with_argv(&[])
}

/// Runs the UCI event loop with the process arguments applied.
///
/// The reference's argv surface is reproduced: `param:Name=Value` entries
/// preload validated search-parameter overrides before the first command, and
/// every other argument is joined into a one-shot UCI command line that runs
/// once and exits (a `quit` is appended). With no arguments the loop reads
/// standard input.
///
/// # Errors
///
/// Returns an I/O error when reading or writing fails, and a fail-fast error
/// for a malformed or unknown `param:` override — the reference aborts the
/// process on those rather than silently searching with default parameters.
pub fn run_with_argv(argv: &[String]) -> io::Result<()> {
    // The `param:` prefix is matched case-insensitively, like the reference.
    let is_param = |argument: &String| argument.to_ascii_lowercase().starts_with("param:");
    let params: Vec<String> = argv
        .iter()
        .filter(|argument| is_param(argument))
        .map(|argument| argument["param:".len()..].to_owned())
        .collect();
    let rest: Vec<&String> = argv.iter().filter(|argument| !is_param(argument)).collect();

    if rest.is_empty() {
        run_from(&mut io::stdin().lock(), &params)
    } else {
        // One-shot mode: the arguments are one UCI line, then the engine exits.
        let command_line = rest
            .iter()
            .map(|argument| argument.as_str())
            .collect::<Vec<&str>>()
            .join(" ");
        let script = format!("{command_line}\nquit\n");
        run_from(&mut script.as_bytes(), &params)
    }
}

/// Runs the UCI event loop over `reader` with preset parameter overrides.
fn run_from<R: BufRead>(reader: &mut R, params: &[String]) -> io::Result<()> {
    let mut output = Output::new();
    // Reference parity (the predecessor's bug-143 fix): the engine starts on
    // the start position, so a `go` before any `position` command searches
    // something sensible instead of answering a null bestmove. The option
    // stays in the type because the answer path below remains as robustness.
    let mut position: Option<Position> = Some(start_position());
    let mut searcher = Searcher::new(DEFAULT_TT_MEGABYTES);
    apply_preset_params(params, &mut searcher, &mut output)?;

    let mut debug_mode = false;
    for line in reader.lines() {
        let line = line?;
        let command = line.trim();
        if command.is_empty() {
            continue;
        }

        match parser::parse(command) {
            parser::Command::Uci => handshake(&mut output)?,
            // The eval-source line is restated on every `isready` (the step-0
            // transcript shows it before each `readyok`): a GUI that attaches
            // late still learns which evaluation the engine will use.
            parser::Command::IsReady => {
                output.line(&eval_source_line())?;
                output.line("readyok")?;
            }
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
                // previous position, so the engine never silently searches a
                // position the GUI did not ask for.
                Err(error) => output.line(&format!("info string {}", error.message()))?,
            },
            parser::Command::Go => {
                handle_go(command, position.as_ref(), &mut searcher, &mut output)?
            }
            // The search runs synchronously until the worker thread lands in
            // step 4; with no search running a `stop` is a no-op, exactly like
            // the reference.
            parser::Command::Stop => {}
            // Ponder support arrives in step 5; without a ponder search a
            // `ponderhit` is ignored, as the step-0 transcript shows.
            parser::Command::PonderHit => {}
            parser::Command::Debug => {
                // The flag exists for reference parity; its consumers arrive
                // with the search instrumentation.
                debug_mode = command
                    .split_whitespace()
                    .nth(1)
                    .is_some_and(|value| value.eq_ignore_ascii_case("on"));
            }
            parser::Command::Perft => handle_perft(command, position.as_ref(), &mut output)?,
            parser::Command::NnueEval => handle_nnue_eval(position.as_ref(), &mut output)?,
            parser::Command::Display => handle_display(position.as_ref(), &mut output)?,
            parser::Command::Eval => handle_eval(position.as_ref(), &mut output)?,
            parser::Command::CheckMoves => handle_check_moves(position.as_ref(), &mut output)?,
            parser::Command::NnueRegions => output.line(
                "info string nnueregions: the king-region instrumentation is not ported yet",
            )?,
            parser::Command::SearchStats => output
                .line("info string searchstats: the search instrumentation is not ported yet")?,
            parser::Command::Quit => break,
            parser::Command::Unknown => {
                // Reference parity: the report names the lowercased command
                // word, not the whole line.
                let word = command.split_whitespace().next().unwrap_or_default();
                output.line(&format!(
                    "info string Unknown command: {}",
                    word.to_ascii_lowercase()
                ))?;
            }
        }
    }
    let _ = debug_mode;

    Ok(())
}

/// Applies `param:Name=Value` startup overrides to the searcher.
///
/// The store and the validation mirror the reference: the name must be a known
/// search lever, a check lever accepts only `true`/`false`, a spin lever only
/// an integer, and a malformed or unknown spec is a fail-fast error (the
/// reference aborts the process rather than searching with default parameters).
/// Every accepted override is announced, because it changes the search.
fn apply_preset_params(
    specs: &[String],
    searcher: &mut Searcher,
    output: &mut Output,
) -> io::Result<()> {
    for spec in specs {
        let Some((name, raw)) = spec.split_once('=') else {
            return Err(io::Error::other(format!(
                "param override '{spec}' is not Name=Value"
            )));
        };
        let Some(lever_spec) = crate::search::options::lever(name) else {
            return Err(io::Error::other(format!(
                "param override: unknown search option '{name}'"
            )));
        };
        let parsed = match lever_spec.kind {
            OptionKind::Check => {
                if raw != "true" && raw != "false" {
                    return Err(io::Error::other(format!(
                        "param override: '{raw}' is not true/false for {name}"
                    )));
                }
                i64::from(raw == "true")
            }
            _ => match raw.parse::<i64>() {
                Ok(value) => value,
                Err(_) => {
                    return Err(io::Error::other(format!(
                        "param override: '{raw}' is not an integer for {name}"
                    )));
                }
            },
        };
        crate::search::options::set(searcher.params_mut(), lever_spec.name, parsed);
        output.line(&format!(
            "info string param-override: {}={raw} (applies to every go)",
            lever_spec.name
        ))?;
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

/// Every limiter a `go` command can carry.
///
/// Unset counters are zero, mirroring the reference's "0 = off" convention:
/// the default-depth rule then reads like the reference's, and unknown tokens
/// are silently ignored, so a GUI that sends an extra keyword never loses its
/// search.
#[derive(Debug, Default, Eq, PartialEq)]
struct GoLimits {
    /// `go depth N` (0 = not given).
    depth: u32,
    /// `go nodes N`.
    nodes: u64,
    /// `go movetime N`.
    movetime: u64,
    /// `go wtime N`.
    white_time: u64,
    /// `go btime N`.
    black_time: u64,
    /// `go winc N`.
    white_inc: u64,
    /// `go binc N`.
    black_inc: u64,
    /// `go movestogo N` (0 = not given).
    moves_to_go: u32,
    /// `go mate N` (0 = not given).
    mate: u32,
    /// `go infinite`.
    infinite: bool,
    /// `go ponder`.
    ponder: bool,
    /// `go perft N` (0 = not given): run perft instead of a search.
    perft_depth: u32,
    /// `go searchmoves` root-move restriction.
    search_moves: Vec<String>,
}

impl GoLimits {
    /// True when a clock limiter (not `movestogo` alone) is present.
    ///
    /// The reference's `HasTimeControl` counts only `movetime` and the two
    /// clocks: `go movestogo 40` alone carries no clock, so it falls through
    /// to the default-depth rule.
    fn has_time_control(&self) -> bool {
        self.movetime > 0 || self.white_time > 0 || self.black_time > 0
    }

    /// Applies the reference's default: a `go` with no limiter at all searches
    /// depth 6.
    fn apply_default_depth(&mut self) {
        if self.depth == 0
            && !self.has_time_control()
            && !self.infinite
            && self.nodes == 0
            && self.perft_depth == 0
            && self.mate == 0
        {
            self.depth = DEFAULT_GO_DEPTH;
        }
    }
}

/// True when `token` looks like a UCI move (4-5 characters, board squares).
///
/// `go searchmoves` consumes exactly these tokens, so a following limiter ends
/// the list. The fifth character (a promotion piece) is not validated, like the
/// reference.
fn is_move_token(token: &str) -> bool {
    let bytes = token.as_bytes();
    let is_square =
        |file: u8, rank: u8| (b'a'..=b'h').contains(&file) && (b'1'..=b'8').contains(&rank);
    (bytes.len() == 4 || bytes.len() == 5)
        && is_square(bytes[0], bytes[1])
        && is_square(bytes[2], bytes[3])
}

/// Parses the limiters of a `go` command into a [`GoLimits`].
///
/// Token matching is case-insensitive and unknown tokens are ignored, exactly
/// like the reference: accepting more input never rejects a valid search. A
/// limiter's value token is consumed even when it does not parse, and the
/// limiter is then simply not set.
fn parse_go(line: &str) -> GoLimits {
    let mut limits = GoLimits::default();
    let mut tokens = line
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .peekable();
    tokens.next(); // the "go" word

    while let Some(token) = tokens.next() {
        match token.as_str() {
            "depth" => {
                if let Some(value) = tokens.next() {
                    limits.depth = value.parse().unwrap_or(0);
                }
            }
            "nodes" => {
                if let Some(value) = tokens.next() {
                    limits.nodes = value.parse().unwrap_or(0);
                }
            }
            "movetime" => {
                if let Some(value) = tokens.next() {
                    limits.movetime = value.parse().unwrap_or(0);
                }
            }
            "wtime" => {
                if let Some(value) = tokens.next() {
                    limits.white_time = value.parse().unwrap_or(0);
                }
            }
            "btime" => {
                if let Some(value) = tokens.next() {
                    limits.black_time = value.parse().unwrap_or(0);
                }
            }
            "winc" => {
                if let Some(value) = tokens.next() {
                    limits.white_inc = value.parse().unwrap_or(0);
                }
            }
            "binc" => {
                if let Some(value) = tokens.next() {
                    limits.black_inc = value.parse().unwrap_or(0);
                }
            }
            "movestogo" => {
                if let Some(value) = tokens.next() {
                    limits.moves_to_go = value.parse().unwrap_or(0);
                }
            }
            "mate" => {
                if let Some(value) = tokens.next() {
                    limits.mate = value.parse().unwrap_or(0);
                }
            }
            "perft" => {
                if let Some(value) = tokens.next() {
                    limits.perft_depth = value.parse().unwrap_or(0);
                }
            }
            "infinite" => limits.infinite = true,
            "ponder" => limits.ponder = true,
            "searchmoves" => {
                // Consume exactly the move-shaped tokens that follow; the first
                // non-move token stays for the main loop, so `go searchmoves
                // e2e4 depth 3` parses both limiters.
                while let Some(next) = tokens.peek() {
                    if !is_move_token(next) {
                        break;
                    }
                    limits.search_moves.push(tokens.next().expect("peeked"));
                }
            }
            // Unknown tokens are ignored (reference parity).
            _ => {}
        }
    }
    limits.apply_default_depth();
    limits
}

/// Searches the current position and prints the UCI transcript.
fn handle_go(
    line: &str,
    position: Option<&Position>,
    searcher: &mut Searcher,
    output: &mut Output,
) -> io::Result<()> {
    let limits = parse_go(line);

    // `go perft N` runs the node counter instead of a search and prints no
    // `bestmove`, exactly like the reference's HandleGo branch.
    if limits.perft_depth > 0 {
        return handle_perft_depth(limits.perft_depth, position, output);
    }

    // Steps 5 and 6 bring the real implementations; until then the fallback
    // answers a legal move so a GUI never waits on a clock (the B32 decision).
    if limits.ponder || limits.mate > 0 || !limits.search_moves.is_empty() {
        output.line(
            "info string go ponder/mate/searchmoves: not supported yet, answering a legal move",
        )?;
        return answer_without_search(position, output);
    }

    let Some(position) = position else {
        output.line("info string no position set")?;
        return answer_without_search(None, output);
    };
    if limits.movetime > 0 {
        searcher.init_movetime(limits.movetime);
    } else if limits.has_time_control() {
        let white_to_move = position.side_to_move_is_white();
        searcher.init_clock(&TimeControl {
            time_left_ms: if white_to_move {
                limits.white_time
            } else {
                limits.black_time
            },
            opponent_time_ms: if white_to_move {
                limits.black_time
            } else {
                limits.white_time
            },
            increment_ms: if white_to_move {
                limits.white_inc
            } else {
                limits.black_inc
            },
            moves_to_go: (limits.moves_to_go > 0).then_some(limits.moves_to_go),
        });
    } else if limits.infinite || limits.depth == 0 && limits.nodes == 0 {
        searcher.init_infinite();
    }

    let mut search_limits = Limits::default();
    if limits.depth > 0 {
        search_limits.depth = Some(limits.depth);
    }
    if limits.nodes > 0 {
        search_limits.nodes = Some(limits.nodes);
    }

    let mut write_error: Option<io::Error> = None;
    let result = searcher.search(position, search_limits, &mut |info| {
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
///
/// Field order and set mirror the captured reference lines exactly:
/// `depth`, `seldepth`, `score`, `nodes`, `nps`, `hashfull`, `time`, `pv`.
/// The reference omits `tbhits` while no tablebases are configured (the
/// capture masks `tbhits <n>` when present, and no captured line carries
/// one); the token joins the line when the Syzygy phase lands.
fn format_info(info: &SearchInfo) -> String {
    let pv = info
        .pv
        .iter()
        .map(|mv| mv.to_uci())
        .collect::<Vec<String>>()
        .join(" ");
    format!(
        "info depth {} seldepth {} score {} nodes {} nps {} hashfull {} time {} pv {}",
        info.depth,
        info.seldepth,
        format_score(info.score),
        info.nodes,
        info.nps,
        info.hashfull,
        info.time_ms,
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
    handle_perft_depth(depth, position, output)
}

/// Counts perft nodes for the current position at `depth`.
fn handle_perft_depth(
    depth: u32,
    position: Option<&Position>,
    output: &mut Output,
) -> io::Result<()> {
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

/// Handles the diagnostic `d` command: position, FEN and zobrist key.
fn handle_display(position: Option<&Position>, output: &mut Output) -> io::Result<()> {
    match position {
        Some(position) => {
            output.line("info string")?;
            output.line(&format!("info string FEN: {}", position.to_fen()))?;
            output.line(&format!("info string Zobrist: 0x{:016X}", position.key()))
        }
        None => output.line("info string No position set"),
    }
}

/// Handles the diagnostic `eval` command: the static evaluation in centipawns.
fn handle_eval(position: Option<&Position>, output: &mut Output) -> io::Result<()> {
    match position {
        Some(position) => output.line(&format!(
            "info string Evaluation: {} cp (source: nnue net {NET_SOURCE_NAME})",
            evaluate_position(position)
        )),
        None => output.line("info string No position set"),
    }
}

/// Handles the diagnostic `checkmoves` command.
///
/// Every legal move is printed with its gives-check verdict, one line each,
/// terminated by `checkmoves done` — the shape the reference's external
/// validation against an authoritative oracle expects.
fn handle_check_moves(position: Option<&Position>, output: &mut Output) -> io::Result<()> {
    let Some(position) = position else {
        output.line("info string No position set")?;
        return output.line("checkmoves done");
    };
    for &mv in legal(position).as_slice() {
        output.line(&format!(
            "checkmove {} givescheck={}",
            mv.to_uci(),
            gives_check(position, mv)
        ))?;
    }
    output.line("checkmoves done")
}

/// Sends the UCI identity block, the option catalogue and `uciok`.
///
/// The three trailing `info string` lines mirror the reference's handshake:
/// the book state, the tablebase state and the evaluation source. The option
/// lines themselves are KnightByte's documented superset of the transcript —
/// the reference keeps its 83 research levers hidden, the ground rules expose
/// the full catalogue as visible options.
fn handshake(output: &mut Output) -> io::Result<()> {
    for line in handshake_lines() {
        output.line(&line)?;
    }
    Ok(())
}

/// Builds every line of the UCI handshake reply.
fn handshake_lines() -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("id name KnightByte {}", env!("CARGO_PKG_VERSION")));
    lines.push("id author KnightByte project".to_owned());
    for option in PUBLIC_OPTIONS {
        let mut line = format!(
            "option name {} type {}",
            option.name,
            kind_name(option.kind)
        );
        // Reference parity: a string option prints its (possibly empty)
        // default, a button prints none.
        if option.kind != OptionKind::Button {
            line.push_str(&format!(" default {}", option.default));
        }
        if let (Some(min), Some(max)) = (option.min, option.max) {
            line.push_str(&format!(" min {min} max {max}"));
        }
        lines.push(line);
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
        lines.push(line);
    }
    // Reference parity: the book, tablebase and eval-source state lines close
    // the handshake, before `uciok`.
    lines.push("info string no opening book loaded".to_owned());
    lines.push("info string syzygy tablebases: none (set SyzygyPath)".to_owned());
    lines.push(eval_source_line());
    lines.push("uciok".to_owned());
    lines
}

/// The `info string` naming the evaluation source (the embedded net file).
///
/// The handshake and every `isready` reply carry it.
fn eval_source_line() -> String {
    format!("info string eval-source: nnue net {NET_SOURCE_NAME}")
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
///
/// Every name is validated against the catalogue (public options and levers);
/// the reply behaviour mirrors the step-0 capture: an unknown name is reported
/// with an `info string`, every value-level rejection (out-of-range spin,
/// non-boolean check value, missing value) is silent and keeps the previous
/// value, and a button acts without a value.
fn handle_set_option(line: &str, searcher: &mut Searcher, output: &mut Output) -> io::Result<()> {
    if let Some(reply) = apply_set_option(line, searcher) {
        output.line(&reply)?;
    }
    Ok(())
}

/// Applies one `setoption` command and returns the optional reply line.
///
/// `None` means the command was accepted silently or rejected silently —
/// both leave no trace on the wire, exactly like the reference.
fn apply_set_option(line: &str, searcher: &mut Searcher) -> Option<String> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.get(1).copied() != Some("name") {
        return Some("info string setoption expects 'name <name>'".to_owned());
    }
    let value_index = tokens.iter().position(|token| *token == "value");
    let (name, value) = match value_index {
        Some(index) => (tokens[2..index].join(" "), tokens[index + 1..].join(" ")),
        None => (tokens[2..].join(" "), String::new()),
    };
    if name.is_empty() {
        return Some("info string setoption needs a name".to_owned());
    }

    if let Some(spec) = public(&name) {
        apply_public_option(spec, &value, searcher);
        return None;
    }
    if let Some(spec) = lever(&name) {
        apply_lever(spec, &value, searcher);
        return None;
    }
    // Reference parity: an unknown name is the one reported rejection.
    Some(format!("info string Unknown option: {name}"))
}

/// True when `value` is a valid spin value for `spec`.
///
/// A value that does not parse, or that falls outside the declared range, is
/// rejected: the previous value is kept and nothing is printed.
fn spin_value_is_valid(spec_min: Option<i64>, spec_max: Option<i64>, value: &str) -> bool {
    match value.parse::<i64>() {
        Ok(parsed) => {
            let min = spec_min.unwrap_or(i64::MIN);
            let max = spec_max.unwrap_or(i64::MAX);
            min <= parsed && parsed <= max
        }
        Err(_) => false,
    }
}

/// Applies one public option with full validation.
///
/// The functional options resize the table (`Hash`), clear it (`Clear Hash`)
/// or set the move overhead; the book, tablebase, `EvalFile` and `Contempt`
/// names are accepted and inert until their phases land. An out-of-range or
/// unparsable spin value is rejected silently and the previous value is kept;
/// a check option accepts only `true`/`false`.
fn apply_public_option(spec: &UciOption, value: &str, searcher: &mut Searcher) {
    match spec.kind {
        OptionKind::Button => {
            // A button acts without a value; a stray value token is ignored.
            if spec.name == "Clear Hash" {
                searcher.clear();
            }
        }
        OptionKind::Check => {
            // Accepted only when well-formed; the inert check options store
            // nothing yet, so acceptance is the whole effect.
        }
        OptionKind::String => {
            // Accepted, inert until the book/tablebase phases land.
        }
        OptionKind::Spin => {
            if !spin_value_is_valid(spec.min, spec.max, value) {
                return;
            }
            match spec.name {
                "Hash" => {
                    searcher.set_hash(value.parse::<usize>().expect("validated above"));
                }
                "Move Overhead" => {
                    searcher.set_move_overhead(value.parse::<u64>().expect("validated above"));
                }
                // `Threads` is validated (1-256) but the engine still searches
                // on one thread until the worker pool lands in step 8.
                _ => {}
            }
        }
    }
}

/// Applies one research lever with full validation.
///
/// A check lever accepts only `true`/`false`; a spin lever only an integer in
/// its declared range. Anything else is rejected silently and the previous
/// value is kept. An unwired lever passes validation and is accepted without
/// effect (see [`crate::search::lever_table`]).
fn apply_lever(spec: &LeverSpec, value: &str, searcher: &mut Searcher) {
    let parsed = match spec.kind {
        OptionKind::Check => {
            if value != "true" && value != "false" {
                return;
            }
            i64::from(value == "true")
        }
        _ => {
            if !spin_value_is_valid(Some(spec.min), Some(spec.max), value) {
                return;
            }
            value.parse::<i64>().expect("validated above")
        }
    };
    set(searcher.params_mut(), spec.name, parsed);
}

#[cfg(test)]
mod tests {
    use super::{GoLimits, apply_set_option, eval_source_line, handshake_lines, parse_go};
    use crate::search::options::{get, lever, public};
    use crate::search::searcher::Searcher;

    /// Applies `line` to a fresh 2 MB searcher and returns (reply, value read
    /// back through the lever accessor).
    fn applied(name: &str, value: Option<&str>) -> (Option<String>, i64) {
        let mut searcher = Searcher::new(2);
        let spec = lever(name).expect("test names a lever");
        let line = match value {
            Some(value) => format!("setoption name {name} value {value}"),
            None => format!("setoption name {name}"),
        };
        let reply = apply_set_option(&line, &mut searcher);
        let state = get(searcher.params(), spec);
        (reply, state)
    }

    #[test]
    fn depth_and_nodes_limiters_parse() {
        let limits = parse_go("go depth 5");
        assert_eq!(limits.depth, 5);
        let limits = parse_go("go nodes 1234");
        assert_eq!(limits.nodes, 1_234);
    }

    #[test]
    fn movetime_becomes_a_fixed_budget() {
        let limits = parse_go("go movetime 100");
        assert_eq!(limits.movetime, 100);
    }

    #[test]
    fn clocks_parse_into_their_fields() {
        let limits = parse_go("go wtime 1000 btime 2000 winc 10 binc 20 movestogo 30");
        assert_eq!(limits.white_time, 1_000);
        assert_eq!(limits.black_time, 2_000);
        assert_eq!(limits.white_inc, 10);
        assert_eq!(limits.black_inc, 20);
        assert_eq!(limits.moves_to_go, 30);
        assert!(limits.has_time_control());
    }

    #[test]
    fn movestogo_alone_is_no_clock_and_defaults_to_depth_six() {
        // Reference parity: `HasTimeControl` does not count `movestogo`, so the
        // default-depth rule fires.
        let limits = parse_go("go movestogo 40");
        assert_eq!(limits.moves_to_go, 40);
        assert!(!limits.has_time_control());
        assert_eq!(limits.depth, 6);
    }

    #[test]
    fn bare_go_defaults_to_depth_six() {
        let limits = parse_go("go");
        assert_eq!(limits.depth, 6);
    }

    #[test]
    fn unknown_tokens_are_ignored() {
        // The reference silently skips what it does not know; a GUI that sends
        // an extra keyword never loses its search.
        let limits = parse_go("go nonsense depth 4 more-nonsense");
        assert_eq!(limits.depth, 4);
    }

    #[test]
    fn limiter_values_are_consumed_even_when_unparsable() {
        // `go depth` with an unparsable value: the value token is consumed and
        // the limiter stays unset, so the default-depth rule fires.
        let limits = parse_go("go depth notanumber");
        assert_eq!(limits.depth, 6);
        let limits = parse_go("go depth");
        assert_eq!(limits.depth, 6);
    }

    #[test]
    fn tokens_are_case_insensitive() {
        let limits = parse_go("GO DEPTH 5 NODES 100 INFINITE");
        assert_eq!(limits.depth, 5);
        assert_eq!(limits.nodes, 100);
        assert!(limits.infinite);
    }

    #[test]
    fn mate_and_ponder_parse() {
        let limits = parse_go("go mate 3");
        assert_eq!(limits.mate, 3);
        assert_eq!(
            limits.depth, 0,
            "a mate limiter suppresses the default depth"
        );
        let limits = parse_go("go ponder");
        assert!(limits.ponder);
        assert_eq!(
            limits.depth, 6,
            "ponder alone does not suppress the default depth"
        );
    }

    #[test]
    fn searchmoves_consumes_move_tokens_until_a_limiter_follows() {
        let limits = parse_go("go searchmoves e2e4 d2d4 depth 3");
        assert_eq!(limits.search_moves, ["e2e4", "d2d4"]);
        assert_eq!(limits.depth, 3);
        // A promotion move is move-shaped; a token that is not a move ends the
        // list and is parsed as a limiter again.
        let limits = parse_go("go searchmoves a7a8q depth 2");
        assert_eq!(limits.search_moves, ["a7a8q"]);
        assert_eq!(limits.depth, 2);
        let limits = parse_go("go searchmoves");
        assert!(limits.search_moves.is_empty());
    }

    #[test]
    fn perft_is_a_go_limiter() {
        let limits = parse_go("go perft 3");
        assert_eq!(limits.perft_depth, 3);
        assert_eq!(limits.depth, 0);
    }

    #[test]
    fn infinite_and_explicit_limiters_suppress_the_default() {
        let limits = parse_go("go infinite");
        assert!(limits.infinite);
        assert_eq!(limits.depth, 0);
        let limits = parse_go("go nodes 1000");
        assert_eq!(limits.depth, 0);
        let limits = parse_go("go movetime 500");
        assert_eq!(limits.depth, 0);
    }

    #[test]
    fn default_construction_matches_the_reference_convention() {
        // Zero means "off" everywhere, which is what makes the default-depth
        // rule read like the reference's.
        let limits = GoLimits::default();
        assert_eq!(limits.depth, 0);
        assert_eq!(limits.nodes, 0);
        assert!(!limits.infinite);
        assert!(!limits.ponder);
    }

    // ------------------------------------------------------------------
    // Phase 5 step 2: one test per `setoption` validation rule. The reply
    // behaviour mirrors the step-0 capture: only an unknown name is reported,
    // every value-level rejection is silent and keeps the previous value.
    // ------------------------------------------------------------------

    #[test]
    fn an_unknown_option_name_is_reported() {
        let mut searcher = Searcher::new(2);
        assert_eq!(
            apply_set_option("setoption name NoSuchOption value 5", &mut searcher),
            Some("info string Unknown option: NoSuchOption".to_owned())
        );
    }

    #[test]
    fn a_spin_value_above_the_maximum_is_rejected_and_the_previous_kept() {
        let (_, set) = applied("AspirationBase", Some("60"));
        assert_eq!(set, 60, "a valid value is applied");
        // 999999 is far above AspirationBase's maximum of 100: rejected, and
        // the previous value survives.
        let (reply, kept) = applied_after("AspirationBase", &["60", "999999"]);
        assert_eq!(reply, None, "a range rejection is silent");
        assert_eq!(kept, 60, "the previous value was not kept");
    }

    #[test]
    fn a_spin_value_below_the_minimum_is_rejected() {
        let (reply, kept) = applied_after("AspirationBase", &["60", "0"]);
        assert_eq!(reply, None);
        assert_eq!(kept, 60, "the previous value was not kept");
    }

    #[test]
    fn an_unparsable_spin_value_is_rejected() {
        let (reply, kept) = applied_after("AspirationBase", &["60", "not-a-number"]);
        assert_eq!(reply, None);
        assert_eq!(kept, 60, "the previous value was not kept");
    }

    #[test]
    fn a_missing_spin_value_is_rejected() {
        let (reply, kept) = applied_after("AspirationBase", &["60", ""]);
        assert_eq!(reply, None);
        assert_eq!(kept, 60, "the previous value was not kept");
    }

    #[test]
    fn a_check_option_accepts_only_true_and_false() {
        // Valid values are applied.
        let (reply, state) = applied("UseEvalCache", Some("true"));
        assert_eq!(reply, None);
        assert_eq!(state, 1);
        // Anything else is rejected silently and the previous value is kept.
        let (reply, kept) = applied_after("UseEvalCache", &["true", "maybe"]);
        assert_eq!(reply, None, "a check rejection is silent");
        assert_eq!(kept, 1, "the previous value was not kept");
        // `false` is accepted again.
        let (reply, state) = applied("UseEvalCache", Some("false"));
        assert_eq!(reply, None);
        assert_eq!(state, 0);
    }

    #[test]
    fn a_button_acts_without_a_value() {
        let mut searcher = Searcher::new(2);
        // Both forms act silently: the bare button and one with a stray value.
        assert_eq!(
            apply_set_option("setoption name Clear Hash", &mut searcher),
            None
        );
        assert_eq!(
            apply_set_option("setoption name Clear Hash value 5", &mut searcher),
            None
        );
    }

    #[test]
    fn hash_values_outside_the_declared_range_are_rejected() {
        let mut searcher = Searcher::new(2);
        for bad in ["0", "999999", "abc"] {
            assert_eq!(
                apply_set_option(&format!("setoption name Hash value {bad}"), &mut searcher),
                None,
                "Hash value {bad} must be rejected silently"
            );
        }
        // A valid value is accepted silently.
        assert_eq!(
            apply_set_option("setoption name Hash value 32", &mut searcher),
            None
        );
    }

    #[test]
    fn threads_is_validated_but_the_engine_stays_single_threaded() {
        let mut searcher = Searcher::new(2);
        // Below the minimum and above the maximum: rejected silently.
        for bad in ["0", "999"] {
            assert_eq!(
                apply_set_option(
                    &format!("setoption name Threads value {bad}"),
                    &mut searcher
                ),
                None,
                "Threads value {bad} must be rejected silently"
            );
        }
        // In range: accepted silently (the pool arrives in step 8).
        for good in ["1", "256"] {
            assert_eq!(
                apply_set_option(
                    &format!("setoption name Threads value {good}"),
                    &mut searcher
                ),
                None,
                "Threads value {good} must be accepted"
            );
        }
    }

    #[test]
    fn reserved_reference_levers_validate_without_effect() {
        // `UseProbCut` ships off and is not ported yet: it is accepted like
        // any other option, but the search state does not change.
        let spec = lever("UseProbCut").expect("the catalogue holds the reserved lever");
        assert!(!spec.wired);
        let (reply, state) = applied("UseProbCut", Some("true"));
        assert_eq!(reply, None);
        assert_eq!(state, spec.default, "a reserved lever must stay inert");
    }

    #[test]
    fn public_inert_options_are_accepted_silently() {
        let mut searcher = Searcher::new(2);
        // Book, tablebase, EvalFile and Contempt names are known (advertised)
        // but inert until their phases land; they must not be reported as
        // unknown.
        for line in [
            "setoption name Own Book value true",
            "setoption name Book File value book.ctg",
            "setoption name Book Depth value 8",
            "setoption name SyzygyPath value C:\\tb",
            "setoption name SyzygyRootProbe value false",
            "setoption name Syzygy50MoveRule value false",
            "setoption name EvalFile value other.nnue",
            "setoption name Contempt value -10",
        ] {
            assert_eq!(
                apply_set_option(line, &mut searcher),
                None,
                "{line} must be accepted silently"
            );
        }
        // Their spin/check values are still validated.
        assert_eq!(
            apply_set_option("setoption name Book Depth value 200", &mut searcher),
            None,
            "an out-of-range inert value must be rejected silently, not applied"
        );
    }

    #[test]
    fn a_malformed_setoption_is_reported() {
        let mut searcher = Searcher::new(2);
        assert!(apply_set_option("setoption value 5", &mut searcher).is_some());
        assert!(apply_set_option("setoption", &mut searcher).is_some());
    }

    #[test]
    fn the_handshake_carries_the_reference_info_lines() {
        let lines = handshake_lines();
        let last = lines.last().map(String::as_str).expect("non-empty");
        assert_eq!(last, "uciok");
        assert!(lines.contains(&"info string no opening book loaded".to_owned()));
        assert!(lines.contains(&"info string syzygy tablebases: none (set SyzygyPath)".to_owned()));
        assert!(lines.contains(&eval_source_line()));
    }

    #[test]
    fn every_public_option_is_resolvable_and_validated() {
        // The public spin ranges are consistent with the validation helper:
        // the default is in range, one step out is not.
        for spec in super::PUBLIC_OPTIONS {
            assert!(public(spec.name).is_some());
            if let (Some(min), Some(max)) = (spec.min, spec.max) {
                assert!(min <= max, "{}: inverted range", spec.name);
            }
        }
    }

    // ------------------------------------------------------------------
    // Phase 5 step 3: `info` line completeness — format tests for the cp
    // and mate scores, the nps arithmetic and the zero-time guard.
    // ------------------------------------------------------------------

    #[test]
    fn scores_format_as_cp_and_mate_distances() {
        use crate::search::searcher::{MATE_LIMIT, MATE_SCORE};
        // Centipawn scores, both signs.
        assert_eq!(super::format_score(45), "cp 45");
        assert_eq!(super::format_score(-123), "cp -123");
        // Mate distances round up to whole moves: two plies are mate in one.
        assert_eq!(super::format_score(MATE_SCORE - 2), "mate 1");
        assert_eq!(super::format_score(-(MATE_SCORE - 2)), "mate -1");
        // An odd ply count still rounds up (three plies are mate in two).
        assert_eq!(super::format_score(MATE_SCORE - 3), "mate 2");
        // Just below the mate limit is still a centipawn score.
        assert_eq!(super::format_score(MATE_LIMIT - 1), "cp 29999");
    }

    #[test]
    fn nps_arithmetic_and_the_zero_time_guard() {
        use crate::search::searcher::nps_of;
        // Ordinary arithmetic: nodes per second, rounded down.
        assert_eq!(nps_of(1_500, 10), 150_000);
        assert_eq!(nps_of(47, 1), 47_000);
        // The guard: a zero elapsed time counts as one millisecond, so a
        // completed iteration is never reported as zero nps and nothing
        // divides by zero.
        assert_eq!(nps_of(47, 0), 47_000);
        assert_eq!(nps_of(0, 0), 0);
    }

    #[test]
    fn the_info_line_carries_every_reference_field_in_order() {
        use crate::search::searcher::{MATE_SCORE, SearchInfo};
        // A captured-shaped iteration: the line must read exactly like the
        // reference's `depth seldepth score nodes nps hashfull time pv`.
        let info = SearchInfo {
            depth: 2,
            seldepth: 3,
            score: 46,
            nodes: 213,
            time_ms: 2,
            nps: 106_500,
            hashfull: 4,
            pv: vec![],
        };
        assert_eq!(
            super::format_info(&info),
            "info depth 2 seldepth 3 score cp 46 nodes 213 nps 106500 hashfull 4 time 2 pv "
        );
        // A mate score keeps the same field set.
        let mate = SearchInfo {
            score: MATE_SCORE - 2,
            ..info.clone()
        };
        let line = super::format_info(&mate);
        assert!(line.contains("score mate 1"), "mate score format: {line}");
        assert!(line.contains("hashfull 4"), "hashfull missing: {line}");
        assert!(line.contains("time 2"), "time missing: {line}");
    }

    /// Applies a sequence of `setoption name <name> value <value>` commands to
    /// one searcher and returns the last (reply, lever state).
    fn applied_after(name: &str, values: &[&str]) -> (Option<String>, i64) {
        let mut searcher = Searcher::new(2);
        let spec = lever(name).expect("test names a lever");
        let mut reply = None;
        for value in values {
            let line = if value.is_empty() {
                format!("setoption name {name}")
            } else {
                format!("setoption name {name} value {value}")
            };
            reply = apply_set_option(&line, &mut searcher);
        }
        (reply, get(searcher.params(), spec))
    }
}
