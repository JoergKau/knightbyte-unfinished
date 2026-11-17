//! Full-refresh network evaluation.
//!
//! The first evaluation path rebuilds both perspective accumulators from the
//! board. It is the correctness oracle for the incremental accumulator added
//! in the performance phase. A reusable [`Evaluator`] keeps the two weight
//! vectors allocated once, so evaluating many positions does not allocate per
//! node.

use std::sync::OnceLock;

use crate::board::position::Position;
use crate::eval::nnue::features::{MAX_ACTIVE_FEATURES, active_features};
use crate::eval::nnue::net::{
    EVAL_SCALE_MUL, EVAL_SCALE_SHIFT, Net, OUTPUT_CAP, OUTPUT_SHIFT, QA, SCRELU_SHIFT,
};
use crate::types::color::Color;

/// A reusable full-refresh evaluator over one network.
pub struct Evaluator<'a> {
    net: &'a Net,
    accumulators: [Vec<i16>; 2],
}

impl<'a> Evaluator<'a> {
    /// Creates an evaluator with two accumulator buffers of the network width.
    pub fn new(net: &'a Net) -> Self {
        let l1 = net.l1();
        Self {
            net,
            accumulators: [vec![0_i16; l1], vec![0_i16; l1]],
        }
    }

    /// Returns the network used by this evaluator.
    pub fn net(&self) -> &Net {
        self.net
    }

    /// Evaluates a position in centipawns from the side-to-move perspective.
    pub fn evaluate(&mut self, position: &Position) -> i32 {
        let stm = position.side();
        let ntm = stm ^ 1;
        self.refresh(position, stm);
        self.refresh(position, ntm);

        let l1 = self.net.l1();
        let output_weights = self.net.output_weights();
        let stm_acc = &self.accumulators[stm as usize];
        let ntm_acc = &self.accumulators[ntm as usize];
        let mut sum = i64::from(self.net.output_bias());

        for index in 0..l1 {
            let a = stm_acc[index];
            if a > 0 {
                sum += i64::from(output(screlu(a))) * i64::from(output_weights[index]);
            }
            let b = ntm_acc[index];
            if b > 0 {
                sum += i64::from(output(screlu(b))) * i64::from(output_weights[l1 + index]);
            }
        }

        ((sum * EVAL_SCALE_MUL) >> EVAL_SCALE_SHIFT) as i32
    }

    fn refresh(&mut self, position: &Position, perspective: Color) {
        let net = self.net;
        let l1 = net.l1();
        let accumulator = &mut self.accumulators[perspective as usize];
        accumulator.copy_from_slice(net.feature_biases());

        let mut features = [0_usize; MAX_ACTIVE_FEATURES];
        let count = active_features(net, position, perspective, &mut features);
        let weights = net.feature_weights();

        for &feature in &features[..count] {
            let row = feature * l1;
            for index in 0..l1 {
                accumulator[index] = accumulator[index].wrapping_add(weights[row + index]);
            }
        }
    }
}

/// SCReLU activation used by the output head.
#[inline(always)]
fn screlu(value: i16) -> i32 {
    let clamped = i32::from(value).clamp(0, QA);
    (clamped * clamped) >> SCRELU_SHIFT
}

/// Output-layer input transform (a no-op clamp for the production format).
#[inline(always)]
fn output(value: i32) -> i32 {
    (value >> OUTPUT_SHIFT).clamp(0, OUTPUT_CAP)
}

static EMBEDDED_NET: OnceLock<Net> = OnceLock::new();

/// Returns the validated embedded production network.
///
/// # Panics
///
/// Panics only if the committed network asset fails validation; the checksum
/// test makes that a build-time packaging error rather than a runtime choice.
pub fn embedded_net() -> &'static Net {
    EMBEDDED_NET.get_or_init(|| {
        Net::from_bytes(crate::tools::net::bytes()).expect("embedded network must be valid")
    })
}

/// Evaluates one position with the embedded network.
///
/// This convenience function is intended for tests and diagnostics. The
/// search will reuse an [`Evaluator`] instead of allocating one per position.
pub fn evaluate_position(position: &Position) -> i32 {
    let net = embedded_net();
    let mut evaluator = Evaluator::new(net);
    evaluator.evaluate(position)
}
