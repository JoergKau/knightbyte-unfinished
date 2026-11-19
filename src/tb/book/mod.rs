//! CTG `.ctg` opening-book support.
//!
//! The book is optional and off by default (`Own Book`). Loading is explicit:
//! the engine maps a path only when `setoption name Book File` names one, or
//! when that option is set empty to request auto-detection. No book file is
//! shipped with the engine.

mod ctg;
mod rng;
mod signature;

pub use ctg::{CtgBook, find_default_book_path};
pub use rng::BookRng;
pub use signature::{compute as compute_signature, is_mirrored};
