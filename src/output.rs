//! Line-oriented UCI output.
//!
//! The search will later publish optional `info` lines through a bounded queue
//! so a slow GUI cannot stall the search thread. The Phase 0 skeleton only
//! needs a small, explicitly flushed writer for protocol replies.

use std::io::{self, Write};

/// Buffered writer for UCI replies on standard output.
pub struct Output {
    stdout: io::Stdout,
}

impl Output {
    /// Creates a writer bound to the process standard output.
    pub fn new() -> Self {
        Self {
            stdout: io::stdout(),
        }
    }

    /// Writes one complete line and flushes it immediately.
    ///
    /// UCI is line-oriented: a reply that stays in a buffer looks like a hung
    /// engine to the GUI, so every line is flushed here.
    pub fn line(&mut self, message: &str) -> io::Result<()> {
        let mut stdout = self.stdout.lock();
        writeln!(stdout, "{message}")?;
        stdout.flush()
    }
}

impl Default for Output {
    fn default() -> Self {
        Self::new()
    }
}
