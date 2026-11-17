//! Embedded v5 neural-network evaluation.

pub mod evaluate;
pub mod features;
pub mod net;

pub use evaluate::{Evaluator, embedded_net, evaluate_position};
pub use net::{Net, NetError};
