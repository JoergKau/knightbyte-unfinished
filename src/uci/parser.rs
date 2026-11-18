//! UCI command recognition.
//!
//! Only the command word is inspected here; arguments are parsed by the
//! handlers in the command loop. Command words are matched case-insensitively
//! (accepting more input never rejects a valid line), matching the reference.

/// A UCI command recognised by the engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    /// Starts the UCI handshake.
    Uci,
    /// Asks whether the engine is ready.
    IsReady,
    /// Resets the engine for a new game.
    UciNewGame,
    /// Runs the deterministic benchmark.
    Bench,
    /// Applies a UCI option.
    SetOption,
    /// Sets the current position for diagnostic commands.
    Position,
    /// Starts a search with the given limiters.
    Go,
    /// Stops a running search (a no-op until the search thread lands in step 4).
    Stop,
    /// Promotes a ponder search to a normal one (a no-op until step 5).
    PonderHit,
    /// Counts perft nodes for the current position.
    Perft,
    /// Prints the network evaluation of the current position.
    NnueEval,
    /// Sets the debug flag (`debug on` / `debug off`).
    Debug,
    /// Prints the current position and its zobrist key (`d`).
    Display,
    /// Prints the static evaluation of the current position.
    Eval,
    /// King-region instrumentation hook of the reference.
    NnueRegions,
    /// Search-instrumentation hook of the reference.
    SearchStats,
    /// Prints every legal move with its gives-check verdict.
    CheckMoves,
    /// Terminates the engine.
    Quit,
    /// Any command that is not implemented yet.
    Unknown,
}

/// Parses the first word of a UCI line into a [`Command`].
///
/// The command word is matched case-insensitively: a GUI may send `GO` or
/// `Quit`, and rejecting a line the engine could understand would only ever
/// lose a game. The remaining arguments belong to the command handlers.
pub fn parse(line: &str) -> Command {
    let word = line
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match word.as_str() {
        "uci" => Command::Uci,
        "isready" => Command::IsReady,
        "ucinewgame" => Command::UciNewGame,
        "bench" => Command::Bench,
        "setoption" => Command::SetOption,
        "position" => Command::Position,
        "go" => Command::Go,
        "stop" => Command::Stop,
        "ponderhit" => Command::PonderHit,
        "perft" => Command::Perft,
        "nnueeval" => Command::NnueEval,
        "debug" => Command::Debug,
        "d" => Command::Display,
        "eval" => Command::Eval,
        "nnueregions" => Command::NnueRegions,
        "searchstats" => Command::SearchStats,
        "checkmoves" => Command::CheckMoves,
        "quit" => Command::Quit,
        _ => Command::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, parse};

    #[test]
    fn recognises_every_protocol_command() {
        // One table entry per variant: every command word the engine knows is
        // recognised, so a GUI speaking full UCI never hits the fallback.
        let table = [
            ("uci", Command::Uci),
            ("isready", Command::IsReady),
            ("ucinewgame", Command::UciNewGame),
            ("bench", Command::Bench),
            ("setoption name Hash value 64", Command::SetOption),
            ("position startpos", Command::Position),
            ("go depth 4", Command::Go),
            ("stop", Command::Stop),
            ("ponderhit", Command::PonderHit),
            ("perft 3", Command::Perft),
            ("nnueeval", Command::NnueEval),
            ("debug on", Command::Debug),
            ("d", Command::Display),
            ("eval", Command::Eval),
            ("nnueregions reset", Command::NnueRegions),
            ("searchstats", Command::SearchStats),
            ("checkmoves", Command::CheckMoves),
            ("quit", Command::Quit),
        ];
        for (line, expected) in table {
            assert_eq!(parse(line), expected, "line: {line}");
        }
    }

    #[test]
    fn command_words_are_case_insensitive() {
        // Accepting more input never rejects a valid line: a GUI that shouts
        // gets the same command as one that does not.
        assert_eq!(parse("UCI"), Command::Uci);
        assert_eq!(parse("Go DEPTH 4"), Command::Go);
        assert_eq!(parse("Stop"), Command::Stop);
        assert_eq!(parse("PONDERHIT"), Command::PonderHit);
        assert_eq!(parse("D"), Command::Display);
        assert_eq!(parse("Quit"), Command::Quit);
    }

    #[test]
    fn ignores_arguments_for_implemented_commands() {
        assert_eq!(parse("bench 16 1"), Command::Bench);
        assert_eq!(parse("uci extra"), Command::Uci);
    }

    #[test]
    fn unknown_and_empty_lines_are_not_commands() {
        assert_eq!(parse("this-command-does-not-exist"), Command::Unknown);
        assert_eq!(parse(""), Command::Unknown);
        assert_eq!(parse("   "), Command::Unknown);
    }
}
