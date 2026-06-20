//! Built-in intrinsic functions.
//!
//! Per phase-3-plan §m1-004: the elaborator resolves `index_*` and
//! `index_*_set` to typed intrinsics with the canonical
//! (*T, u64) -> T !{RawMem} @{paideia.raw_mem} signature.

/// Type classification for intrinsic signatures.
///
/// Phase-3 simplification: the elaborator's intrinsics use a simplified
/// TypeKind enum to represent the 10 width families + pointer variants.
/// A future PR may unify this with TypeId once the elaborator's
/// type-environment surface stabilizes.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum TypeKind {
    /// Pointer to u8.
    PtrU8,
    /// Pointer to u16.
    PtrU16,
    /// Pointer to u32.
    PtrU32,
    /// Pointer to u64.
    PtrU64,
    /// Pointer to i8.
    PtrI8,
    /// Pointer to i16.
    PtrI16,
    /// Pointer to i32.
    PtrI32,
    /// Pointer to i64.
    PtrI64,
    /// Pointer to f32.
    PtrF32,
    /// Pointer to f64.
    PtrF64,
    /// 8-bit unsigned integer.
    U8,
    /// 16-bit unsigned integer.
    U16,
    /// 32-bit unsigned integer.
    U32,
    /// 64-bit unsigned integer.
    U64,
    /// 8-bit signed integer.
    I8,
    /// 16-bit signed integer.
    I16,
    /// 32-bit signed integer.
    I32,
    /// 64-bit signed integer.
    I64,
    /// 32-bit floating-point.
    F32,
    /// 64-bit floating-point.
    F64,
    /// Index type (u64).
    UIndex,
    /// Unit type.
    Unit,
}

/// Signature of an intrinsic function.
///
/// All `index_*` and `index_*_set` intrinsics share a canonical effect/capability row:
/// - effect_row = "RawMem"
/// - capability_row = "paideia.raw_mem"
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntrinsicSignature {
    /// The intrinsic's name (e.g., "index_u64", "index_i8_set").
    pub name: String,
    /// Parameter types in order.
    pub param_types: Vec<TypeKind>,
    /// Return type.
    pub return_type: TypeKind,
    /// Effect row (canonical form "RawMem" for phase-3-m1-004).
    pub effect_row: String,
    /// Capability row (canonical form "paideia.raw_mem").
    pub capability_row: String,
}

impl IntrinsicSignature {
    /// Create an intrinsic signature with the canonical effect/capability rows.
    #[must_use]
    fn canonical(name: String, param_types: Vec<TypeKind>, return_type: TypeKind) -> Self {
        Self {
            name,
            param_types,
            return_type,
            effect_row: "RawMem".to_string(),
            capability_row: "paideia.raw_mem".to_string(),
        }
    }
}

/// Construct the list of all intrinsic signatures.
///
/// Generates 40 intrinsics:
/// - 10 width families (u8, u16, u32, u64, i8, i16, i32, i64, f32, f64).
/// - Each family has:
///   - `index_*` (2 params: ptr + index) and `index_*_set` (3 params: ptr + index + value).
///   - `ptr_sub_*` (2 params: ptr + ptr) → element distance (u64).
///   - `ptr_sub_bytes_*` (2 params: ptr + ptr) → byte distance (u64).
#[must_use]
pub fn all_intrinsics() -> Vec<IntrinsicSignature> {
    let width_configs = [
        ("u8", TypeKind::U8, TypeKind::PtrU8),
        ("u16", TypeKind::U16, TypeKind::PtrU16),
        ("u32", TypeKind::U32, TypeKind::PtrU32),
        ("u64", TypeKind::U64, TypeKind::PtrU64),
        ("i8", TypeKind::I8, TypeKind::PtrI8),
        ("i16", TypeKind::I16, TypeKind::PtrI16),
        ("i32", TypeKind::I32, TypeKind::PtrI32),
        ("i64", TypeKind::I64, TypeKind::PtrI64),
        ("f32", TypeKind::F32, TypeKind::PtrF32),
        ("f64", TypeKind::F64, TypeKind::PtrF64),
    ];

    let mut out = Vec::with_capacity(width_configs.len() * 4);

    for (name, _val_t, ptr_t) in &width_configs {
        // index_* : (*T, u64) -> T
        out.push(IntrinsicSignature::canonical(
            format!("index_{name}"),
            vec![*ptr_t, TypeKind::UIndex],
            *_val_t,
        ));

        // index_*_set : (*T, u64, T) -> ()
        out.push(IntrinsicSignature::canonical(
            format!("index_{name}_set"),
            vec![*ptr_t, TypeKind::UIndex, *_val_t],
            TypeKind::Unit,
        ));

        // ptr_sub_* : (*T, *T) -> u64 (element distance)
        // Effect-free: pointer subtraction is register-only.
        out.push(IntrinsicSignature {
            name: format!("ptr_sub_{name}"),
            param_types: vec![*ptr_t, *ptr_t],
            return_type: TypeKind::U64,
            effect_row: String::new(),
            capability_row: String::new(),
        });

        // ptr_sub_bytes_* : (*T, *T) -> u64 (byte distance)
        // Effect-free: pointer subtraction is register-only.
        out.push(IntrinsicSignature {
            name: format!("ptr_sub_bytes_{name}"),
            param_types: vec![*ptr_t, *ptr_t],
            return_type: TypeKind::U64,
            effect_row: String::new(),
            capability_row: String::new(),
        });
    }

    out
}

/// Look up an intrinsic by name.
///
/// Returns the signature if the name matches a registered intrinsic,
/// or `None` if the name is unknown.
#[must_use]
pub fn lookup_intrinsic(name: &str) -> Option<IntrinsicSignature> {
    all_intrinsics().into_iter().find(|i| i.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_intrinsics_returns_40_entries() {
        let intrinsics = all_intrinsics();
        assert_eq!(
            intrinsics.len(),
            40,
            "expected 10 widths × 4 (get + set + ptr_sub + ptr_sub_bytes) = 40 intrinsics"
        );
    }

    #[test]
    fn index_u64_signature_is_ptr_u64_uindex_to_u64() {
        let sig = lookup_intrinsic("index_u64").expect("index_u64 should exist");
        assert_eq!(sig.name, "index_u64");
        assert_eq!(sig.param_types.len(), 2);
        assert_eq!(sig.param_types[0], TypeKind::PtrU64);
        assert_eq!(sig.param_types[1], TypeKind::UIndex);
        assert_eq!(sig.return_type, TypeKind::U64);
        assert_eq!(sig.effect_row, "RawMem");
        assert_eq!(sig.capability_row, "paideia.raw_mem");
    }

    #[test]
    fn index_u8_set_signature_is_ptr_u8_uindex_u8_to_unit() {
        let sig = lookup_intrinsic("index_u8_set").expect("index_u8_set should exist");
        assert_eq!(sig.name, "index_u8_set");
        assert_eq!(sig.param_types.len(), 3);
        assert_eq!(sig.param_types[0], TypeKind::PtrU8);
        assert_eq!(sig.param_types[1], TypeKind::UIndex);
        assert_eq!(sig.param_types[2], TypeKind::U8);
        assert_eq!(sig.return_type, TypeKind::Unit);
        assert_eq!(sig.effect_row, "RawMem");
        assert_eq!(sig.capability_row, "paideia.raw_mem");
    }

    #[test]
    fn lookup_intrinsic_finds_index_u32() {
        let sig = lookup_intrinsic("index_u32").expect("index_u32 should exist");
        assert_eq!(sig.name, "index_u32");
        assert_eq!(sig.return_type, TypeKind::U32);
    }

    #[test]
    fn lookup_intrinsic_returns_none_for_unknown() {
        let result = lookup_intrinsic("unknown_function");
        assert!(
            result.is_none(),
            "lookup_intrinsic should return None for unknown names"
        );
    }

    #[test]
    fn all_intrinsic_names_are_unique() {
        let intrinsics = all_intrinsics();
        let mut names = std::collections::HashSet::new();
        for sig in &intrinsics {
            assert!(
                names.insert(&sig.name),
                "duplicate intrinsic name: {}",
                sig.name
            );
        }
    }

    #[test]
    fn index_i64_set_has_canonical_row() {
        let sig = lookup_intrinsic("index_i64_set").expect("index_i64_set should exist");
        assert_eq!(sig.effect_row, "RawMem");
        assert_eq!(sig.capability_row, "paideia.raw_mem");
    }

    #[test]
    fn index_f64_has_correct_types() {
        let sig = lookup_intrinsic("index_f64").expect("index_f64 should exist");
        assert_eq!(sig.return_type, TypeKind::F64);
        assert_eq!(sig.param_types[0], TypeKind::PtrF64);
        assert_eq!(sig.param_types[1], TypeKind::UIndex);
    }

    #[test]
    fn intrinsic_set_operations_return_unit() {
        let all = all_intrinsics();
        let set_ops: Vec<_> = all.iter().filter(|s| s.name.contains("_set")).collect();
        for sig in set_ops {
            assert_eq!(
                sig.return_type,
                TypeKind::Unit,
                "{} should return Unit",
                sig.name
            );
        }
    }

    #[test]
    fn intrinsic_get_operations_return_value_type() {
        let all = all_intrinsics();
        let get_ops: Vec<_> = all.iter().filter(|s| !s.name.contains("_set")).collect();
        for sig in get_ops {
            assert_ne!(
                sig.return_type,
                TypeKind::Unit,
                "{} should not return Unit",
                sig.name
            );
            // Verify the value type is not a pointer
            assert!(
                !matches!(
                    sig.return_type,
                    TypeKind::PtrU8
                        | TypeKind::PtrU16
                        | TypeKind::PtrU32
                        | TypeKind::PtrU64
                        | TypeKind::PtrI8
                        | TypeKind::PtrI16
                        | TypeKind::PtrI32
                        | TypeKind::PtrI64
                        | TypeKind::PtrF32
                        | TypeKind::PtrF64
                ),
                "{} should return a value type, not a pointer",
                sig.name
            );
        }
    }

    #[test]
    fn ptr_sub_signature_returns_u64() {
        let sig = lookup_intrinsic("ptr_sub_u64").expect("ptr_sub_u64 should exist");
        assert_eq!(sig.name, "ptr_sub_u64");
        assert_eq!(sig.param_types.len(), 2);
        assert_eq!(sig.param_types[0], TypeKind::PtrU64);
        assert_eq!(sig.param_types[1], TypeKind::PtrU64);
        assert_eq!(sig.return_type, TypeKind::U64);
        assert_eq!(sig.effect_row, "");
        assert_eq!(sig.capability_row, "");
    }

    #[test]
    fn ptr_sub_bytes_signature_returns_u64() {
        let sig = lookup_intrinsic("ptr_sub_bytes_u64").expect("ptr_sub_bytes_u64 should exist");
        assert_eq!(sig.name, "ptr_sub_bytes_u64");
        assert_eq!(sig.param_types.len(), 2);
        assert_eq!(sig.param_types[0], TypeKind::PtrU64);
        assert_eq!(sig.param_types[1], TypeKind::PtrU64);
        assert_eq!(sig.return_type, TypeKind::U64);
        assert_eq!(sig.effect_row, "");
        assert_eq!(sig.capability_row, "");
    }

    #[test]
    fn ptr_sub_count_is_20_entries() {
        let all = all_intrinsics();
        let ptr_sub_count = all.iter().filter(|s| s.name.contains("ptr_sub")).count();
        assert_eq!(
            ptr_sub_count, 20,
            "expected 10 widths × (ptr_sub + ptr_sub_bytes) = 20 ptr_sub* entries"
        );
    }

    #[test]
    fn lookup_intrinsic_finds_ptr_sub_u64() {
        let sig = lookup_intrinsic("ptr_sub_u64").expect("ptr_sub_u64 should exist");
        assert_eq!(sig.name, "ptr_sub_u64");
        assert_eq!(sig.return_type, TypeKind::U64);
        assert_eq!(sig.param_types[0], TypeKind::PtrU64);
    }
}
