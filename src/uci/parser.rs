//! UCI command recognition.
//!
//! Only the command word is inspected here; arguments are parsed by the
//! handlers in the command loop.

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
    /// Counts perft nodes for the current position.
    Perft,
    /// Prints the network evaluation of the current position.
    NnueEval,
    /// Terminates the engine.
    Quit,
    /// Any command that is not implemented yet.
    Unknown,
}

/// Parses the first word of a UCI line into a [`Command`].
///
/// The parser is case-sensitive, as required by UCI, and ignores the remaining
/// arguments until the corresponding command is implemented.
pub fn parse(line: &str) -> Command {
    match line.split_whitespace().next().unwrap_or_default() {
        "uci" => Command::Uci,
        "isready" => Command::IsReady,
        "ucinewgame" => Command::UciNewGame,
        "bench" => Command::Bench,
        "setoption" => Command::SetOption,
        "position" => Command::Position,
        "go" => Command::Go,
        "perft" => Command::Perft,
        "nnueeval" => Command::NnueEval,
        "quit" => Command::Quit,
        _ => Command::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, parse};

    #[test]
    fn recognises_protocol_commands() {
        assert_eq!(parse("uci"), Command::Uci);
        assert_eq!(parse("isready"), Command::IsReady);
        assert_eq!(parse("ucinewgame"), Command::UciNewGame);
        assert_eq!(parse("bench"), Command::Bench);
        assert_eq!(parse("setoption name Hash value 64"), Command::SetOption);
        assert_eq!(parse("position startpos"), Command::Position);
        assert_eq!(parse("go depth 4"), Command::Go);
        assert_eq!(parse("perft 3"), Command::Perft);
        assert_eq!(parse("nnueeval"), Command::NnueEval);
        assert_eq!(parse("quit"), Command::Quit);
    }

    #[test]
    fn ignores_arguments_for_implemented_commands() {
        assert_eq!(parse("bench 16 1"), Command::Bench);
        assert_eq!(parse("uci extra"), Command::Uci);
    }

    #[test]
    fn unknown_and_empty_lines_are_not_commands() {
        assert_eq!(parse("stop"), Command::Unknown);
        assert_eq!(parse(""), Command::Unknown);
        assert_eq!(parse("   "), Command::Unknown);
    }
}
