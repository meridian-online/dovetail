//! dovetail-core — the modelling layer's reusable library: detection, the plan
//! model, and emission. survey discovers how to load unfamiliar data; this
//! crate holds the detection layer that decision rests on.
//!
//! The core is pure Rust and holds all behaviour; the `dovetail` CLI is a thin
//! layer over it.

pub mod datapackage;
pub mod detect;
pub mod emit;
pub mod eval;
pub mod finetype_ext;
pub mod identifiers;
pub mod nominations;
pub mod relate;
pub mod structure;
pub mod survey;
pub mod transform;
pub mod typing;

pub use detect::{Detector, SampledInput, ShapeHeuristicDetector};
pub use nominations::{Nomination, Nominations};
pub use structure::{Column, Detection, Format, Structure};

#[cfg(feature = "finetype-guided")]
pub use detect::FinetypeGuidedDetector;
