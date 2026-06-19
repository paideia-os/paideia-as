//! Optimization passes for the ELF emitter.
//!
//! These passes operate at the code-layout level (after IR lowering) to improve
//! generated ELF object file quality. Passes include macro-fusion alignment,
//! code placement, and other low-level optimizations.
//!
//! Note: The IR opt module in paideia-as-ir is for IR-level passes. These are
//! emitter-level passes, which are invoked during the emission phase.

pub mod encode_tight;
pub mod macro_fusion;

pub use encode_tight::EncodeTightPass;
pub use macro_fusion::MacroFusionPass;
