//! KnightByte engine library.
//!
//! The binary is intentionally thin: it starts the UCI loop, while the
//! reusable engine components live in this library so they can be unit-tested
//! without spawning a process.

pub mod board;
pub mod eval;
pub mod output;
pub mod platform;
pub mod search;
pub mod tools;
pub mod types;
pub mod uci;
