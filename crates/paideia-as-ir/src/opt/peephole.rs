//! Peephole optimization pass.
//!
//! Local pattern rewrites on the IR's instruction stream. Nine canonical
//! rewrites — see [`PeepholeRewrite`] for the catalog. v0.36.47
//! (PAS-DEBT-B3-004, #1517) lifted three of the four stubs to real
//! rewrites (`strength-reduce-mul`, `combine-push-pop` two forms), and
//! carried the div-by-power-of-2 and jump-to-next patterns to
//! B3-004-b as a documented deferral (both need infrastructure the
//! peephole window doesn't have — data-flow across the div triple
//! and label→instruction position tracking respectively).

use super::{OptDiagSink, OptPass};
use crate::IrArena;
use crate::instruction::{Mnemonic, Operand, RegId};
use crate::node::IrNodeId;
use smallvec::SmallVec;

#[cfg(test)]
use crate::instruction::InstrMode;

/// The peephole optimization pass.
pub struct PeepholePass;

/// The nine canonical peephole rewrites.
///
/// 1. `RemoveNopMov` — `mov r, r` → eliminate.
/// 2. `SimplifyZeroAdd` — `add r, 0` → eliminate.
/// 3. `SimplifyZeroSub` — `sub r, 0` → eliminate.
/// 4. `StrengthReduceMul` — `imul r, r, imm_pow2` → `shl r, log2(imm)`
///    (v0.36.47, #1517).
/// 5. `StrengthReduceDiv` — `div r_pow2` (with prior `mov r_pow2, imm`)
///    → `shr rax, log2(imm)`. Deferred to B3-004-b — needs data-flow
///    across the div triple `mov r,imm; xor rdx,rdx; div r`.
/// 6. `FuseLoadStore` — `mov r, [m]; mov [m], r` → eliminate.
/// 7. `CollapseJumpToNext` — `jmp L` where L labels the next
///    instruction → eliminate. Deferred to B3-004-b — needs a
///    label→instruction-position side table the peephole doesn't own.
/// 8. `CombinePushPop` — `push X; pop X` → eliminate;
///    `push X; pop Y` → `mov Y, X` (v0.36.47, #1517).
/// 9. `CompareToTest` — `cmp r, 0; jcc(Eq|Ne|Zero|NonZero)` →
///    `test r, r; jcc …`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum PeepholeRewrite {
    RemoveNopMov,
    SimplifyZeroAdd,
    SimplifyZeroSub,
    StrengthReduceMul,
    StrengthReduceDiv,
    FuseLoadStore,
    CollapseJumpToNext,
    CombinePushPop,
    CompareToTest,
}

impl PeepholeRewrite {
    /// Canonical rewrite name for diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            Self::RemoveNopMov => "remove-nop-mov",
            Self::SimplifyZeroAdd => "simplify-zero-add",
            Self::SimplifyZeroSub => "simplify-zero-sub",
            Self::StrengthReduceMul => "strength-reduce-mul",
            Self::StrengthReduceDiv => "strength-reduce-div",
            Self::FuseLoadStore => "fuse-load-store",
            Self::CollapseJumpToNext => "collapse-jump-to-next",
            Self::CombinePushPop => "combine-push-pop",
            Self::CompareToTest => "compare-to-test",
        }
    }

    /// All nine canonical rewrites, in catalog order.
    pub fn all() -> &'static [PeepholeRewrite] {
        &[
            Self::RemoveNopMov,
            Self::SimplifyZeroAdd,
            Self::SimplifyZeroSub,
            Self::StrengthReduceMul,
            Self::StrengthReduceDiv,
            Self::FuseLoadStore,
            Self::CollapseJumpToNext,
            Self::CombinePushPop,
            Self::CompareToTest,
        ]
    }
}

/// One pending mutation to apply to the instruction table after the
/// pattern-matching borrow is released.
///
/// Keeping every rewrite's follow-up in a single enum lets the dispatch
/// loop read as a flat match against outcomes rather than a fan of
/// Option slots per rewrite kind.
enum Pending {
    /// Delete these instruction ids (1 or 2). `name` labels the rewrite
    /// in the diagnostic so the fused/removed sites stay distinguishable.
    Remove {
        ids: SmallVec<[IrNodeId; 2]>,
        name: PeepholeRewrite,
    },
    /// Rewrite the `cmp` at `id` in place to `test r, r`.
    ReplaceCmpWithTest { id: IrNodeId },
    /// Rewrite the `imul r, r, imm` at `id` in place to `shl r, shift`.
    ReplaceImulWithShl { id: IrNodeId, shift: u8 },
    /// Collapse `push src; pop dst` (different regs) into `mov dst, src`:
    /// mutate `push_id` in place, remove `pop_id`.
    ReplacePushPopWithMov {
        push_id: IrNodeId,
        pop_id: IrNodeId,
        dst: RegId,
        src: RegId,
    },
}

/// Try `mov r, r` → eliminate.
fn try_remove_nop_mov(
    table: &crate::instruction::InstructionSideTable,
    ids: &[IrNodeId],
) -> Option<Pending> {
    let inst = table.get(*ids.first()?)?;
    if inst.mnemonic != Mnemonic::Mov || inst.operands.len() != 2 {
        return None;
    }
    match (&inst.operands[0], &inst.operands[1]) {
        (Operand::Reg(a), Operand::Reg(b)) if a == b => Some(Pending::Remove {
            ids: SmallVec::from_slice(&[ids[0]]),
            name: PeepholeRewrite::RemoveNopMov,
        }),
        _ => None,
    }
}

/// Try `add r, 0` → eliminate.
fn try_simplify_zero_add(
    table: &crate::instruction::InstructionSideTable,
    ids: &[IrNodeId],
) -> Option<Pending> {
    let inst = table.get(*ids.first()?)?;
    if inst.mnemonic != Mnemonic::Add || inst.operands.len() != 2 {
        return None;
    }
    matches!(&inst.operands[1], Operand::Imm64(0)).then(|| Pending::Remove {
        ids: SmallVec::from_slice(&[ids[0]]),
        name: PeepholeRewrite::SimplifyZeroAdd,
    })
}

/// Try `sub r, 0` → eliminate.
fn try_simplify_zero_sub(
    table: &crate::instruction::InstructionSideTable,
    ids: &[IrNodeId],
) -> Option<Pending> {
    let inst = table.get(*ids.first()?)?;
    if inst.mnemonic != Mnemonic::Sub || inst.operands.len() != 2 {
        return None;
    }
    matches!(&inst.operands[1], Operand::Imm64(0)).then(|| Pending::Remove {
        ids: SmallVec::from_slice(&[ids[0]]),
        name: PeepholeRewrite::SimplifyZeroSub,
    })
}

/// Try `imul r, r, imm_pow2` → `shl r, log2(imm)`.
///
/// Only fires on the 3-operand form where `dst == src` and `imm` is a
/// positive power of two whose `log2` fits `shl r64, imm8`. Different
/// dst/src would need a `mov` insertion, which erodes the strength
/// reduction (imul reg,reg,imm8 = 4 bytes ≤ mov + shl = 7 bytes).
///
/// `Mul` in the runtime enum is the wide unsigned `mul r64` (implicit
/// rax, no immediate operand) — no strength-reduce shape exists for it.
fn try_strength_reduce_mul(
    table: &crate::instruction::InstructionSideTable,
    ids: &[IrNodeId],
) -> Option<Pending> {
    let inst = table.get(*ids.first()?)?;
    if inst.mnemonic != Mnemonic::Imul || inst.operands.len() != 3 {
        return None;
    }
    let (dst, src, imm) = match (&inst.operands[0], &inst.operands[1], &inst.operands[2]) {
        (Operand::Reg(d), Operand::Reg(s), Operand::Imm64(i)) => (*d, *s, *i),
        _ => return None,
    };
    if dst != src || imm <= 0 {
        return None;
    }
    let u = imm as u64;
    if !u.is_power_of_two() {
        return None;
    }
    let shift = u.trailing_zeros();
    if shift == 0 || shift > 63 {
        return None; // shift==0 means imm==1, no rewrite; >63 out-of-range for shl r64,imm8
    }
    Some(Pending::ReplaceImulWithShl {
        id: ids[0],
        shift: shift as u8,
    })
}

/// Try `mov r, [m]; mov [m], r` → eliminate.
fn try_fuse_load_store(
    table: &crate::instruction::InstructionSideTable,
    ids: &[IrNodeId],
) -> Option<Pending> {
    if ids.len() < 2 {
        return None;
    }
    let inst0 = table.get(ids[0])?;
    let inst1 = table.get(ids[1])?;
    if inst0.mnemonic != Mnemonic::Mov || inst0.operands.len() != 2 {
        return None;
    }
    let (reg, mem) = match (&inst0.operands[0], &inst0.operands[1]) {
        (Operand::Reg(r), Operand::MemSib { .. } | Operand::MemDisp { .. }) => {
            (r, &inst0.operands[1])
        }
        _ => return None,
    };
    if inst1.mnemonic != Mnemonic::Mov || inst1.operands.len() != 2 {
        return None;
    }
    match (&inst1.operands[0], &inst1.operands[1]) {
        (Operand::MemSib { .. } | Operand::MemDisp { .. }, Operand::Reg(r2))
            if r2 == reg && mem == &inst1.operands[0] =>
        {
            Some(Pending::Remove {
                ids: SmallVec::from_slice(&[ids[0], ids[1]]),
                name: PeepholeRewrite::FuseLoadStore,
            })
        }
        _ => None,
    }
}

/// Try `push X; pop Y` → `mov Y, X` (or eliminate both when X == Y).
///
/// Excludes RSP: pushing/popping RSP has stack-frame semantics the
/// peephole should not paper over.
fn try_combine_push_pop(
    table: &crate::instruction::InstructionSideTable,
    ids: &[IrNodeId],
) -> Option<Pending> {
    if ids.len() < 2 {
        return None;
    }
    let inst0 = table.get(ids[0])?;
    let inst1 = table.get(ids[1])?;
    if inst0.mnemonic != Mnemonic::Push || inst0.operands.len() != 1 {
        return None;
    }
    if inst1.mnemonic != Mnemonic::Pop || inst1.operands.len() != 1 {
        return None;
    }
    let (src, dst) = match (&inst0.operands[0], &inst1.operands[0]) {
        (Operand::Reg(s), Operand::Reg(d)) => (*s, *d),
        _ => return None,
    };
    // RSP = 4. Skip stack-pointer plays.
    if src == RegId(4) || dst == RegId(4) {
        return None;
    }
    if src == dst {
        Some(Pending::Remove {
            ids: SmallVec::from_slice(&[ids[0], ids[1]]),
            name: PeepholeRewrite::CombinePushPop,
        })
    } else {
        Some(Pending::ReplacePushPopWithMov {
            push_id: ids[0],
            pop_id: ids[1],
            dst,
            src,
        })
    }
}

/// Try `cmp r, 0; jcc(Eq|Ne|Zero|NonZero)` → mutate `cmp` to `test r, r`
/// (PA-R14-011). `jcc` is untouched — same flags read.
fn try_compare_to_test(
    table: &crate::instruction::InstructionSideTable,
    ids: &[IrNodeId],
) -> Option<Pending> {
    if ids.len() < 2 {
        return None;
    }
    let inst0 = table.get(ids[0])?;
    let inst1 = table.get(ids[1])?;
    if inst0.mnemonic != Mnemonic::Cmp || inst0.operands.len() != 2 {
        return None;
    }
    if !matches!(
        (&inst0.operands[0], &inst0.operands[1]),
        (Operand::Reg(_), Operand::Imm64(0))
    ) {
        return None;
    }
    match inst1.mnemonic {
        Mnemonic::Jcc(cond) => match cond {
            crate::instruction::Cond::Eq
            | crate::instruction::Cond::Ne
            | crate::instruction::Cond::Zero
            | crate::instruction::Cond::NonZero => Some(Pending::ReplaceCmpWithTest { id: ids[0] }),
            _ => None,
        },
        _ => None,
    }
}

impl OptPass for PeepholePass {
    fn name(&self) -> &'static str {
        "peephole"
    }

    fn apply(&self, arena: &mut IrArena, _function_root: IrNodeId, sink: &mut OptDiagSink) -> bool {
        let mut ids: Vec<IrNodeId> = {
            let table = arena.instructions();
            table.entries().keys().copied().collect()
        };
        if ids.is_empty() {
            return false;
        }
        // IrNodeIds are handed out sequentially → sort preserves program order.
        ids.sort_by_key(|id| id.get());

        let mut changed = false;
        let mut i = 0;
        while i < ids.len() {
            let remaining = &ids[i..];

            // Try each rewrite in catalog order under a scoped read borrow.
            let pending: Option<Pending> = {
                let table = arena.instructions();
                try_remove_nop_mov(table, remaining)
                    .or_else(|| try_simplify_zero_add(table, remaining))
                    .or_else(|| try_simplify_zero_sub(table, remaining))
                    .or_else(|| try_strength_reduce_mul(table, remaining))
                    .or_else(|| try_fuse_load_store(table, remaining))
                    .or_else(|| try_combine_push_pop(table, remaining))
                    .or_else(|| try_compare_to_test(table, remaining))
            };

            match pending {
                None => {
                    i += 1;
                }
                Some(Pending::Remove { ids: remove_ids, name }) => {
                    let n = remove_ids.len();
                    let diag = if n == 1 {
                        format!("O1501 ({}): i{}", name.name(), remove_ids[0].get())
                    } else {
                        format!(
                            "O1502 ({}): i{} + i{}",
                            name.name(),
                            remove_ids[0].get(),
                            remove_ids[1].get()
                        )
                    };
                    sink.emit("peephole", diag);
                    {
                        let table = arena.instructions_mut();
                        for id in &remove_ids {
                            table.remove(*id);
                        }
                    }
                    changed = true;
                    // Skip past all removed positions.
                    i += n;
                }
                Some(Pending::ReplaceCmpWithTest { id }) => {
                    sink.emit(
                        "peephole",
                        format!("O1503 (compare-to-test): i{}", id.get()),
                    );
                    if let Some(inst) = arena.instructions_mut().get_mut(id) {
                        if let Operand::Reg(r) = inst.operands[0] {
                            inst.mnemonic = Mnemonic::Test;
                            inst.operands[1] = Operand::Reg(r);
                            changed = true;
                        }
                    }
                    i += 1;
                }
                Some(Pending::ReplaceImulWithShl { id, shift }) => {
                    sink.emit(
                        "peephole",
                        format!(
                            "O1512 (strength-reduce-mul): i{} imul→shl {}",
                            id.get(),
                            shift
                        ),
                    );
                    if let Some(inst) = arena.instructions_mut().get_mut(id) {
                        if let Operand::Reg(dst) = inst.operands[0] {
                            inst.mnemonic = Mnemonic::Shl;
                            inst.operands.clear();
                            inst.operands.push(Operand::Reg(dst));
                            inst.operands.push(Operand::Imm64(shift as i64));
                            changed = true;
                        }
                    }
                    i += 1;
                }
                Some(Pending::ReplacePushPopWithMov {
                    push_id,
                    pop_id,
                    dst,
                    src,
                }) => {
                    sink.emit(
                        "peephole",
                        format!(
                            "O1513 (combine-push-pop): i{} + i{} → mov",
                            push_id.get(),
                            pop_id.get()
                        ),
                    );
                    {
                        let table = arena.instructions_mut();
                        if let Some(inst) = table.get_mut(push_id) {
                            inst.mnemonic = Mnemonic::Mov;
                            inst.operands.clear();
                            inst.operands.push(Operand::Reg(dst));
                            inst.operands.push(Operand::Reg(src));
                        }
                        table.remove(pop_id);
                    }
                    changed = true;
                    i += 2;
                }
            }
        }

        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instruction::{Cond, Instruction};
    use smallvec::SmallVec;

    fn mk(mnemonic: Mnemonic, ops: &[Operand]) -> Instruction {
        let mut operands: SmallVec<[Operand; 3]> = SmallVec::new();
        for o in ops {
            operands.push(o.clone());
        }
        Instruction {
            mnemonic,
            operands,
            encoding_hint: None,
            byte_offset_in_text: None,
            mode: InstrMode::default(),
            emission_order: 0,
        }
    }

    fn insert(arena: &mut IrArena, id: u32, inst: Instruction) -> IrNodeId {
        let node = IrNodeId::new(id).unwrap();
        arena.instructions_mut().insert(node, inst);
        node
    }

    #[test]
    fn peephole_rewrite_names_are_unique() {
        let names: Vec<&str> = PeepholeRewrite::all().iter().map(|r| r.name()).collect();
        let unique = names.iter().collect::<std::collections::HashSet<_>>().len();
        assert_eq!(names.len(), unique);
    }

    #[test]
    fn peephole_pass_emits_no_diagnostics_for_empty_arena() {
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let changed = PeepholePass.apply(&mut arena, IrNodeId::new(1).unwrap(), &mut sink);
        assert!(!changed);
        assert!(sink.diagnostics.is_empty());
    }

    #[test]
    fn peephole_pass_removes_nop_mov() {
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let id = insert(
            &mut arena,
            1,
            mk(Mnemonic::Mov, &[Operand::Reg(RegId(0)), Operand::Reg(RegId(0))]),
        );
        let changed = PeepholePass.apply(&mut arena, id, &mut sink);
        assert!(changed);
        assert!(arena.instructions().get(id).is_none());
    }

    #[test]
    fn peephole_pass_name_is_peephole() {
        assert_eq!(PeepholePass.name(), "peephole");
    }

    #[test]
    fn peephole_rewrite_all_returns_nine() {
        assert_eq!(PeepholeRewrite::all().len(), 9);
    }

    // ── strength-reduce-mul (v0.36.47, #1517) ────────────────────────

    #[test]
    fn strength_reduce_mul_rewrites_pow2_immediate() {
        // imul rax, rax, 8  →  shl rax, 3
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let id = insert(
            &mut arena,
            1,
            mk(
                Mnemonic::Imul,
                &[
                    Operand::Reg(RegId(0)),
                    Operand::Reg(RegId(0)),
                    Operand::Imm64(8),
                ],
            ),
        );
        let changed = PeepholePass.apply(&mut arena, id, &mut sink);
        assert!(changed);
        let inst = arena.instructions().get(id).unwrap();
        assert_eq!(inst.mnemonic, Mnemonic::Shl);
        assert_eq!(inst.operands.len(), 2);
        assert_eq!(inst.operands[0], Operand::Reg(RegId(0)));
        assert_eq!(inst.operands[1], Operand::Imm64(3));
        assert!(sink.diagnostics[0].message.contains("strength-reduce-mul"));
    }

    #[test]
    fn strength_reduce_mul_handles_large_pow2() {
        // imul rax, rax, 1<<62 → shl rax, 62
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let id = insert(
            &mut arena,
            1,
            mk(
                Mnemonic::Imul,
                &[
                    Operand::Reg(RegId(3)),
                    Operand::Reg(RegId(3)),
                    Operand::Imm64(1i64 << 62),
                ],
            ),
        );
        let changed = PeepholePass.apply(&mut arena, id, &mut sink);
        assert!(changed);
        let inst = arena.instructions().get(id).unwrap();
        assert_eq!(inst.mnemonic, Mnemonic::Shl);
        assert_eq!(inst.operands[1], Operand::Imm64(62));
    }

    #[test]
    fn strength_reduce_mul_skips_non_pow2() {
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let id = insert(
            &mut arena,
            1,
            mk(
                Mnemonic::Imul,
                &[
                    Operand::Reg(RegId(0)),
                    Operand::Reg(RegId(0)),
                    Operand::Imm64(6),
                ],
            ),
        );
        let changed = PeepholePass.apply(&mut arena, id, &mut sink);
        assert!(!changed);
        assert_eq!(arena.instructions().get(id).unwrap().mnemonic, Mnemonic::Imul);
    }

    #[test]
    fn strength_reduce_mul_skips_negative_and_zero_and_one() {
        for imm in [-2i64, -8, 0, 1] {
            let mut arena = IrArena::new();
            let mut sink = OptDiagSink::new();
            let id = insert(
                &mut arena,
                1,
                mk(
                    Mnemonic::Imul,
                    &[
                        Operand::Reg(RegId(0)),
                        Operand::Reg(RegId(0)),
                        Operand::Imm64(imm),
                    ],
                ),
            );
            let changed = PeepholePass.apply(&mut arena, id, &mut sink);
            assert!(!changed, "imm {imm} must not rewrite");
            assert_eq!(
                arena.instructions().get(id).unwrap().mnemonic,
                Mnemonic::Imul
            );
        }
    }

    #[test]
    fn strength_reduce_mul_skips_when_dst_ne_src() {
        // imul rax, rbx, 4 — different dst/src; mov+shl would be worse.
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let id = insert(
            &mut arena,
            1,
            mk(
                Mnemonic::Imul,
                &[
                    Operand::Reg(RegId(0)),
                    Operand::Reg(RegId(3)),
                    Operand::Imm64(4),
                ],
            ),
        );
        let changed = PeepholePass.apply(&mut arena, id, &mut sink);
        assert!(!changed);
        assert_eq!(arena.instructions().get(id).unwrap().mnemonic, Mnemonic::Imul);
    }

    #[test]
    fn strength_reduce_mul_skips_two_operand_form() {
        // imul rax, rbx (2-operand) — no immediate present.
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let id = insert(
            &mut arena,
            1,
            mk(
                Mnemonic::Imul,
                &[Operand::Reg(RegId(0)), Operand::Reg(RegId(3))],
            ),
        );
        let changed = PeepholePass.apply(&mut arena, id, &mut sink);
        assert!(!changed);
    }

    // ── combine-push-pop (v0.36.47, #1517) ───────────────────────────

    #[test]
    fn combine_push_pop_same_reg_eliminates_both() {
        // push rax; pop rax → (nothing)
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let push = insert(
            &mut arena,
            1,
            mk(Mnemonic::Push, &[Operand::Reg(RegId(0))]),
        );
        let pop = insert(&mut arena, 2, mk(Mnemonic::Pop, &[Operand::Reg(RegId(0))]));
        let changed = PeepholePass.apply(&mut arena, push, &mut sink);
        assert!(changed);
        assert!(arena.instructions().get(push).is_none());
        assert!(arena.instructions().get(pop).is_none());
    }

    #[test]
    fn combine_push_pop_different_regs_becomes_mov() {
        // push rax; pop rbx → mov rbx, rax; (pop removed)
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let push = insert(
            &mut arena,
            1,
            mk(Mnemonic::Push, &[Operand::Reg(RegId(0))]),
        );
        let pop = insert(&mut arena, 2, mk(Mnemonic::Pop, &[Operand::Reg(RegId(3))]));
        let changed = PeepholePass.apply(&mut arena, push, &mut sink);
        assert!(changed);
        let mv = arena.instructions().get(push).unwrap();
        assert_eq!(mv.mnemonic, Mnemonic::Mov);
        assert_eq!(mv.operands[0], Operand::Reg(RegId(3))); // dst = pop_reg
        assert_eq!(mv.operands[1], Operand::Reg(RegId(0))); // src = push_reg
        assert!(arena.instructions().get(pop).is_none());
    }

    #[test]
    fn combine_push_pop_skips_rsp() {
        // push rsp; pop rax — RSP has stack-frame semantics; leave alone.
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let push = insert(
            &mut arena,
            1,
            mk(Mnemonic::Push, &[Operand::Reg(RegId(4))]),
        );
        let pop = insert(&mut arena, 2, mk(Mnemonic::Pop, &[Operand::Reg(RegId(0))]));
        let changed = PeepholePass.apply(&mut arena, push, &mut sink);
        assert!(!changed);
        assert_eq!(arena.instructions().get(push).unwrap().mnemonic, Mnemonic::Push);
        assert_eq!(arena.instructions().get(pop).unwrap().mnemonic, Mnemonic::Pop);
    }

    #[test]
    fn combine_push_pop_skips_when_pop_target_is_rsp() {
        // push rax; pop rsp — never rewrite.
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        insert(&mut arena, 1, mk(Mnemonic::Push, &[Operand::Reg(RegId(0))]));
        insert(&mut arena, 2, mk(Mnemonic::Pop, &[Operand::Reg(RegId(4))]));
        let changed = PeepholePass.apply(&mut arena, IrNodeId::new(1).unwrap(), &mut sink);
        assert!(!changed);
    }

    #[test]
    fn combine_push_pop_skips_non_reg_operand() {
        // push [rbp-8]; pop rax — memory push is not the pattern we handle.
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        insert(
            &mut arena,
            1,
            mk(
                Mnemonic::Push,
                &[Operand::MemSib {
                    base: RegId(5),
                    index: None,
                    scale: crate::instruction::Scale::X1,
                    disp: -8,
                }],
            ),
        );
        insert(&mut arena, 2, mk(Mnemonic::Pop, &[Operand::Reg(RegId(0))]));
        let changed = PeepholePass.apply(&mut arena, IrNodeId::new(1).unwrap(), &mut sink);
        assert!(!changed);
    }

    // ── deferred rewrites (B3-004-b) — verify no spurious rewrites ───

    #[test]
    fn strength_reduce_div_is_deferred_no_rewrite() {
        // Div r64 exists; no attempt is made to rewrite it here.
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let id = insert(
            &mut arena,
            1,
            mk(Mnemonic::Div, &[Operand::Reg(RegId(3))]),
        );
        let changed = PeepholePass.apply(&mut arena, id, &mut sink);
        assert!(!changed);
        assert_eq!(arena.instructions().get(id).unwrap().mnemonic, Mnemonic::Div);
    }

    #[test]
    fn collapse_jump_to_next_is_deferred_no_rewrite() {
        // A bare Jmp without a label→position table stays put.
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let id = insert(
            &mut arena,
            1,
            mk(
                Mnemonic::Jmp,
                &[Operand::LabelRef {
                    name: "L".to_string(),
                    addend: 0,
                }],
            ),
        );
        let changed = PeepholePass.apply(&mut arena, id, &mut sink);
        assert!(!changed);
        assert_eq!(arena.instructions().get(id).unwrap().mnemonic, Mnemonic::Jmp);
    }

    // ── compare-to-test (existing, kept green) ───────────────────────

    #[test]
    fn peephole_pass_rewrites_cmp_to_test() {
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let cmp = insert(
            &mut arena,
            1,
            mk(
                Mnemonic::Cmp,
                &[Operand::Reg(RegId(0)), Operand::Imm64(0)],
            ),
        );
        insert(
            &mut arena,
            2,
            mk(
                Mnemonic::Jcc(Cond::Eq),
                &[Operand::LabelRef {
                    name: "L".to_string(),
                    addend: 0,
                }],
            ),
        );
        let changed = PeepholePass.apply(&mut arena, cmp, &mut sink);
        assert!(changed);
        let mutated = arena.instructions().get(cmp).unwrap();
        assert_eq!(mutated.mnemonic, Mnemonic::Test);
        assert_eq!(mutated.operands[0], Operand::Reg(RegId(0)));
        assert_eq!(mutated.operands[1], Operand::Reg(RegId(0)));
    }

    #[test]
    fn peephole_pass_skips_cmp_when_imm_not_zero() {
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let cmp = insert(
            &mut arena,
            1,
            mk(
                Mnemonic::Cmp,
                &[Operand::Reg(RegId(0)), Operand::Imm64(1)],
            ),
        );
        insert(
            &mut arena,
            2,
            mk(
                Mnemonic::Jcc(Cond::Eq),
                &[Operand::LabelRef {
                    name: "L".to_string(),
                    addend: 0,
                }],
            ),
        );
        let changed = PeepholePass.apply(&mut arena, cmp, &mut sink);
        assert!(!changed);
        assert_eq!(arena.instructions().get(cmp).unwrap().mnemonic, Mnemonic::Cmp);
    }

    #[test]
    fn peephole_pass_skips_cmp_when_next_not_jcc() {
        // cmp rax, 0; mov rbx, 1  — no jcc follower ⇒ no rewrite.
        let mut arena = IrArena::new();
        let mut sink = OptDiagSink::new();
        let cmp = insert(
            &mut arena,
            1,
            mk(
                Mnemonic::Cmp,
                &[Operand::Reg(RegId(0)), Operand::Imm64(0)],
            ),
        );
        insert(
            &mut arena,
            2,
            mk(
                Mnemonic::Mov,
                &[Operand::Reg(RegId(3)), Operand::Imm64(1)],
            ),
        );
        let changed = PeepholePass.apply(&mut arena, cmp, &mut sink);
        assert!(!changed);
        assert_eq!(arena.instructions().get(cmp).unwrap().mnemonic, Mnemonic::Cmp);
    }

    #[test]
    fn peephole_pass_rewrites_cmp_to_test_with_four_conditions() {
        for cond in [Cond::Eq, Cond::Ne, Cond::Zero, Cond::NonZero] {
            let mut arena = IrArena::new();
            let mut sink = OptDiagSink::new();
            let cmp = insert(
                &mut arena,
                1,
                mk(
                    Mnemonic::Cmp,
                    &[Operand::Reg(RegId(1)), Operand::Imm64(0)],
                ),
            );
            insert(
                &mut arena,
                2,
                mk(
                    Mnemonic::Jcc(cond),
                    &[Operand::LabelRef {
                        name: "L".to_string(),
                        addend: 0,
                    }],
                ),
            );
            let changed = PeepholePass.apply(&mut arena, cmp, &mut sink);
            assert!(changed, "cond {cond:?} must rewrite");
            assert_eq!(
                arena.instructions().get(cmp).unwrap().mnemonic,
                Mnemonic::Test
            );
        }
    }
}
