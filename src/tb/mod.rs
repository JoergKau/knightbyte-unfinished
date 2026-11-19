//! Endgame and opening knowledge outside the search itself.
//!
//! Two optional features live here:
//!
//! * [`syzygy`] — probing of Syzygy WDL/DTZ tablebases (3-4-5 piece sets and
//!   larger sets, up to seven pieces).
//! * [`book`] — reading and probing CTG `.ctg` opening books.
//!
//! Both are loadable at runtime through `setoption`; with no path set they cost
//! nothing, and every probe fails soft instead of panicking on missing or
//! malformed data.

pub mod book;
pub mod syzygy;
