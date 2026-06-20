//! The detection layer. A [`Detector`] reads a sampled input and reports its
//! [`Detection`] — format, row-level structure, and columns. Detection is the
//! make-or-break capability for survey (spec 2026-06-20-survey-detection-and-load):
//! a wrong structure means the emitted recipe loads the wrong table.
//!
//! Two implementations ship for the head-to-head eval (ac-02 / ac-03):
//! - [`ShapeHeuristicDetector`] — pure structural inspection, no finetype.
//! - `FinetypeGuidedDetector` (feature `finetype-guided`) — finetype-model's
//!   column classifier driving a recursive descent into nested structure.

mod sample;
mod shape;

#[cfg(feature = "finetype-guided")]
mod finetype_guided;

pub use sample::{SampleError, SampledInput};
pub use shape::ShapeHeuristicDetector;

#[cfg(feature = "finetype-guided")]
pub use finetype_guided::FinetypeGuidedDetector;

use crate::structure::Detection;

/// A detection strategy. Implementations are compared head-to-head on the
/// fixture corpus by the eval harness; the winner backs survey's emission.
pub trait Detector {
    /// A short, stable identifier used in eval output and rung reporting.
    fn name(&self) -> &str;

    /// Detect the format, row-level structure, and columns of a sampled input.
    fn detect(&self, input: &SampledInput) -> Detection;
}
