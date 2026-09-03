//! Structured scope markers for prefix-attribute blocks (v0.28-M1 primitives).
//!
//! A `BlockScope` is a category tag attached to a statement-block body that
//! records *how* that block was written in source — the syntactic prefix
//! that scoped it. The elaborator consumes this to reconstitute an implicit
//! effect / capability discipline over the block without re-parsing.
//!
//! Wave 0 Batch 2 (v0.28-M1) introduces `@gpu_context(engine) { … }` as the
//! first family member (paideia-as#1370). Sibling batches — `@gpu_kernel`,
//! `@transfer`, `@barrier` — append variants to this enum. Each variant
//! carries its own payload struct so that the parser lands rich per-scope
//! data (engine reference, launch geometry, direction, etc.) without
//! smearing fields across a single flat variant.
//!
//! **Serialization point.** Multiple Wave 0 Batch 2 primitives extend this
//! enum in parallel; conflicts on the enum body are expected and are
//! resolved by keeping every appended variant. Do not reshape existing
//! variants without a coordinated rev bump — downstream elaborator crates
//! pattern-match on the variant list.
//!
//! **Parser-only landing (Wave 0).** The parser produces these payloads
//! and hands them to callers; the elaborator wiring that stamps the
//! implicit effect row (`GpuSubmit`, `GpuLaunch`, …) ships in v0.29-M1.

use crate::NodeId;

/// Payload for a `@gpu_context(engine) { stmts }` block (paideia-as#1370).
///
/// `engine` is the parsed expression that resolves to a `Cap<KIND_GPU_ENGINE>`
/// value — the engine handle every submission inside the body borrows. It is
/// stored as a `NodeId` pointing at the arena-allocated expression node so
/// that later passes can rewalk the expression without re-parsing.
///
/// `body` is the `NodeId` of the block expression (`NodeKind::ExprBlock`)
/// that wraps the statement list. Storing the block as a whole (rather than
/// a flat `Vec<NodeId>` of statements) preserves the block's span for
/// diagnostics and keeps the tail/statement distinction visible to the
/// elaborator when it stamps the implicit GPU-submission effect row.
///
/// # Nesting
///
/// The parser rejects nested `@gpu_context` blocks (single-level rule): the
/// implicit submission effect is scoped to exactly one dynamic-extent frame,
/// and stacking frames would either mask the outer engine's discipline or
/// silently pick one — both are surprising. The rejection surfaces as
/// `P0293` from the parser.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuContextBlock {
    /// Engine expression: must elaborate to `Cap<KIND_GPU_ENGINE>` in the
    /// v0.29-M1 wiring step. `NodeId` of any expression node.
    pub engine: NodeId,
    /// Body block: a `NodeKind::ExprBlock` node whose statements elaborate
    /// with the implicit GPU-submission effect row.
    pub body: NodeId,
}

/// Category tag for statement-block bodies opened by a prefix attribute.
///
/// Each variant carries its own payload struct so per-scope metadata (engine
/// handle, kernel geometry, transfer direction, …) travels with the marker
/// rather than living in a parallel side-table keyed by the block `NodeId`.
///
/// **Extension point.** Sibling Wave 0 Batch 2 primitives append variants
/// (e.g. `Kernel(GpuKernelBlock)`, `Transfer(TransferBlock)`, …). Keep the
/// enum `#[non_exhaustive]` so downstream matches remain forward-compatible
/// while later batches land.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum BlockScope {
    /// `@gpu_context(engine) { … }` — implicit GPU-submission scope.
    ///
    /// paideia-as#1370, v0.28-M1-001. Wire-up of the implicit effect row is
    /// deferred to v0.29-M1-001; the parser only records the payload here.
    GpuContext(GpuContextBlock),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nid(n: u32) -> NodeId {
        NodeId::new(n).unwrap()
    }

    #[test]
    fn gpu_context_block_round_trips_through_variant() {
        let payload = GpuContextBlock {
            engine: nid(7),
            body: nid(11),
        };
        let scope = BlockScope::GpuContext(payload.clone());
        match scope {
            BlockScope::GpuContext(inner) => assert_eq!(inner, payload),
        }
    }
}
