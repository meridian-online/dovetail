//! dovetail-core — the modelling layer's reusable library: detection, the plan
//! model, and emission. survey discovers how to load unfamiliar data; this
//! crate holds the detection layer that decision rests on.
//!
//! Per choice 0008 (pure Rust core under a thin CLI) and spec
//! 2026-06-20-survey-detection-and-load.

pub mod datapackage;
pub mod detect;
pub mod emit;
pub mod eval;
pub mod structure;
pub mod survey;

pub use detect::{Detector, SampledInput, ShapeHeuristicDetector};
pub use structure::{Column, Detection, Format, Structure};

#[cfg(feature = "finetype-guided")]
pub use detect::FinetypeGuidedDetector;
