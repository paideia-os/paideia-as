//! Memory layout computation for types.
//!
//! Determines the size and alignment of types, accounting for padding
//! and alignment requirements of record fields. Used by the code generator
//! and linker to correctly position data.

use crate::intern::TypeInterner;
use crate::types::{EnumPayload, Type, TypeId};

/// Size and alignment requirements for a type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Size in bytes.
    pub size: u64,
    /// Alignment requirement in bytes (power of 2).
    pub alignment: u64,
}

impl Layout {
    /// Create a layout with explicit size and alignment.
    pub fn new(size: u64, alignment: u64) -> Self {
        Self { size, alignment }
    }
}

/// Compute the memory layout of a type.
///
/// For primitive types, returns their standard sizes (u8=1, u16=2, u32=4, u64=8, ptr=8).
/// For record types, computes layout with field alignment and padding.
pub fn layout_of(interner: &TypeInterner, ty: TypeId) -> Layout {
    match interner.get(ty) {
        Type::Unit => Layout {
            size: 0,
            alignment: 1,
        },
        Type::Bool => Layout {
            size: 1,
            alignment: 1,
        },
        Type::Char => Layout {
            size: 4,
            alignment: 4,
        },
        Type::UInt(16) => Layout {
            size: 2,
            alignment: 2,
        },
        Type::UInt(32) => Layout {
            size: 4,
            alignment: 4,
        },
        Type::UInt(64) | Type::UInt(0xFFFF) => Layout {
            size: 8,
            alignment: 8,
        },
        Type::UInt(128) => Layout {
            size: 16,
            alignment: 16,
        },
        Type::UInt(8) => Layout {
            size: 1,
            alignment: 1,
        },
        Type::UInt(_) => Layout {
            size: 8,
            alignment: 8,
        },
        Type::SInt(16) => Layout {
            size: 2,
            alignment: 2,
        },
        Type::SInt(32) => Layout {
            size: 4,
            alignment: 4,
        },
        Type::SInt(64) | Type::SInt(0xFFFF) => Layout {
            size: 8,
            alignment: 8,
        },
        Type::SInt(128) => Layout {
            size: 16,
            alignment: 16,
        },
        Type::SInt(8) => Layout {
            size: 1,
            alignment: 1,
        },
        Type::SInt(_) => Layout {
            size: 8,
            alignment: 8,
        },
        Type::Float(32) => Layout {
            size: 4,
            alignment: 4,
        },
        Type::Float(64) => Layout {
            size: 8,
            alignment: 8,
        },
        Type::Float(_) => Layout {
            size: 8,
            alignment: 8,
        },
        Type::Ptr { .. } => Layout {
            size: 8,
            alignment: 8,
        },
        Type::Record { fields } => layout_of_record(interner, fields),
        Type::Enum { variants } => layout_of_enum(interner, variants),
        // Conservative defaults for types without explicit size info
        _ => Layout {
            size: 8,
            alignment: 8,
        },
    }
}

/// Compute layout for a record type.
///
/// Fields are laid out sequentially with padding inserted between fields
/// to satisfy alignment requirements. The total size is padded to a multiple
/// of the maximum field alignment (tail padding).
fn layout_of_record(
    interner: &TypeInterner,
    fields: &smallvec::SmallVec<[(u32, TypeId); 4]>,
) -> Layout {
    if fields.is_empty() {
        return Layout {
            size: 0,
            alignment: 1,
        };
    }

    let mut offset = 0u64;
    let mut max_align = 1u64;

    for (_name, field_ty) in fields {
        let field_layout = layout_of(interner, *field_ty);

        // Align offset to field's alignment requirement
        let padding =
            (field_layout.alignment - (offset % field_layout.alignment)) % field_layout.alignment;
        offset += padding;
        offset += field_layout.size;

        if field_layout.alignment > max_align {
            max_align = field_layout.alignment;
        }
    }

    // Tail-pad to maximum field alignment
    let tail_padding = (max_align - (offset % max_align)) % max_align;
    let size = offset + tail_padding;

    Layout {
        size,
        alignment: max_align,
    }
}

/// Compute layout for an enum (tagged union) type.
///
/// Layout consists of:
/// 1. Discriminant (8 bytes, 8-byte aligned) — tags which variant is active
/// 2. Payload (variable size based on largest variant)
///
/// The total size is the discriminant (8 bytes) plus padding to payload alignment,
/// plus the maximum payload size, plus tail padding to alignment boundary.
fn layout_of_enum(
    interner: &TypeInterner,
    variants: &smallvec::SmallVec<[(u32, EnumPayload); 4]>,
) -> Layout {
    // Discriminant: 8 bytes, 8-byte aligned
    let discriminant_size = 8u64;
    let discriminant_align = 8u64;

    // Compute maximum payload size and alignment
    let mut max_payload_size = 0u64;
    let mut max_payload_align = 1u64;

    for (_name, payload) in variants {
        let payload_layout = layout_of_payload(interner, payload);
        if payload_layout.size > max_payload_size {
            max_payload_size = payload_layout.size;
        }
        if payload_layout.alignment > max_payload_align {
            max_payload_align = payload_layout.alignment;
        }
    }

    // Total layout: discriminant + payload (with padding/alignment)
    let payload_offset = align_up(discriminant_size, max_payload_align);
    let body_end = payload_offset + max_payload_size;
    let total_align = discriminant_align.max(max_payload_align);
    let size = align_up(body_end, total_align);

    Layout {
        size,
        alignment: total_align,
    }
}

/// Compute layout for an enum variant payload.
fn layout_of_payload(interner: &TypeInterner, payload: &EnumPayload) -> Layout {
    match payload {
        EnumPayload::Unit => Layout {
            size: 0,
            alignment: 1,
        },
        EnumPayload::Tuple(types) => layout_of_tuple_payload(interner, types),
        EnumPayload::Record(fields) => layout_of_record_payload(interner, fields),
    }
}

/// Compute layout for a tuple variant payload.
fn layout_of_tuple_payload(
    interner: &TypeInterner,
    types: &smallvec::SmallVec<[TypeId; 4]>,
) -> Layout {
    if types.is_empty() {
        return Layout {
            size: 0,
            alignment: 1,
        };
    }

    let mut offset = 0u64;
    let mut max_align = 1u64;

    for ty in types {
        let ty_layout = layout_of(interner, *ty);

        // Align offset to type's alignment requirement
        let padding = (ty_layout.alignment - (offset % ty_layout.alignment)) % ty_layout.alignment;
        offset += padding;
        offset += ty_layout.size;

        if ty_layout.alignment > max_align {
            max_align = ty_layout.alignment;
        }
    }

    // Tail-pad to maximum alignment
    let tail_padding = (max_align - (offset % max_align)) % max_align;
    let size = offset + tail_padding;

    Layout {
        size,
        alignment: max_align,
    }
}

/// Compute layout for a record variant payload.
fn layout_of_record_payload(
    interner: &TypeInterner,
    fields: &smallvec::SmallVec<[(u32, TypeId); 4]>,
) -> Layout {
    layout_of_record(interner, fields)
}

/// Align a value up to the next alignment boundary.
///
/// If `offset` is already aligned, returns `offset` unchanged.
/// Otherwise, returns the next multiple of `align` that is >= `offset`.
#[inline]
fn align_up(offset: u64, align: u64) -> u64 {
    if align == 0 {
        return offset;
    }
    (offset + align - 1) & !(align - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Type;

    #[test]
    fn record_layout_simple() {
        let mut interner = TypeInterner::new();
        let u8_id = interner.uint(8);
        let u64_id = interner.uint(64);

        // record { x: u8, y: u64 }
        // Layout: u8 (1 byte) + 7 bytes padding + u64 (8 bytes) = 16 bytes, align 8
        let record_id = interner.intern(Type::Record {
            fields: smallvec::smallvec![(1, u8_id), (2, u64_id)],
        });

        let layout = layout_of(&interner, record_id);
        assert_eq!(layout.size, 16, "Expected size 16 (1 + 7 padding + 8)");
        assert_eq!(layout.alignment, 8, "Expected alignment 8 (from u64)");
    }

    #[test]
    fn record_layout_empty() {
        let interner = TypeInterner::new();
        let fields = smallvec::smallvec![];
        let layout = layout_of_record(&interner, &fields);
        assert_eq!(layout.size, 0, "Empty record should have size 0");
        assert_eq!(layout.alignment, 1, "Empty record should have alignment 1");
    }

    #[test]
    fn record_layout_single_field() {
        let mut interner = TypeInterner::new();
        let u32_id = interner.uint(32);

        let record_id = interner.intern(Type::Record {
            fields: smallvec::smallvec![(1, u32_id)],
        });

        let layout = layout_of(&interner, record_id);
        assert_eq!(layout.size, 4, "Single u32 field should have size 4");
        assert_eq!(
            layout.alignment, 4,
            "Single u32 field should have alignment 4"
        );
    }

    #[test]
    fn record_layout_with_pointer() {
        let mut interner = TypeInterner::new();
        let u32_id = interner.uint(32);
        let u64_id = interner.uint(64);
        let ptr_u64_id = interner.intern(Type::Ptr {
            pointee: u64_id,
            mutable: false,
        });

        // record { a: u32, b: *u64 }
        // Layout: u32 (4 bytes) + 4 bytes padding + ptr (8 bytes) = 16 bytes, align 8
        let record_id = interner.intern(Type::Record {
            fields: smallvec::smallvec![(1, u32_id), (2, ptr_u64_id)],
        });

        let layout = layout_of(&interner, record_id);
        assert_eq!(layout.size, 16, "Expected size 16 (4 + 4 padding + 8)");
        assert_eq!(layout.alignment, 8, "Expected alignment 8 (from pointer)");
    }

    #[test]
    fn record_layout_nested() {
        let mut interner = TypeInterner::new();
        let u8_id = interner.uint(8);
        let u64_id = interner.uint(64);

        // Inner record: record { x: u8 }
        // Layout: size 1, align 1
        let inner_record_id = interner.intern(Type::Record {
            fields: smallvec::smallvec![(1, u8_id)],
        });

        // Outer record: record { a: inner, b: u64 }
        // Layout: inner (1 byte) + 7 bytes padding + u64 (8 bytes) = 16 bytes, align 8
        let outer_record_id = interner.intern(Type::Record {
            fields: smallvec::smallvec![(2, inner_record_id), (3, u64_id)],
        });

        let layout = layout_of(&interner, outer_record_id);
        assert_eq!(layout.size, 16, "Expected nested record size 16");
        assert_eq!(layout.alignment, 8, "Expected nested record alignment 8");
    }

    #[test]
    fn primitive_layout_u8() {
        let mut interner = TypeInterner::new();
        let u8_id = interner.uint(8);
        let layout = layout_of(&interner, u8_id);
        assert_eq!(layout.size, 1);
        assert_eq!(layout.alignment, 1);
    }

    #[test]
    fn primitive_layout_u64() {
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);
        let layout = layout_of(&interner, u64_id);
        assert_eq!(layout.size, 8);
        assert_eq!(layout.alignment, 8);
    }

    #[test]
    fn primitive_layout_unit() {
        let mut interner = TypeInterner::new();
        let unit_id = interner.unit();
        let layout = layout_of(&interner, unit_id);
        assert_eq!(layout.size, 0);
        assert_eq!(layout.alignment, 1);
    }

    #[test]
    fn enum_layout_pure_tag() {
        // enum { A, B, C } — only discriminant, no payload
        let mut interner = TypeInterner::new();
        let enum_id = interner.intern(Type::Enum {
            variants: smallvec::smallvec![
                (1, EnumPayload::Unit),
                (2, EnumPayload::Unit),
                (3, EnumPayload::Unit),
            ],
        });

        let layout = layout_of(&interner, enum_id);
        assert_eq!(
            layout.size, 8,
            "Pure tag enum should have size 8 (just discriminant)"
        );
        assert_eq!(layout.alignment, 8, "Pure tag enum should have alignment 8");
    }

    #[test]
    fn enum_layout_tuple_payload() {
        // enum { Some(u64), None } — discriminant + u64 payload
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);

        let enum_id = interner.intern(Type::Enum {
            variants: smallvec::smallvec![
                (1, EnumPayload::Tuple(smallvec::smallvec![u64_id])),
                (2, EnumPayload::Unit),
            ],
        });

        let layout = layout_of(&interner, enum_id);
        // Layout: 8 bytes discriminant + 8 bytes payload = 16 bytes, align 8
        assert_eq!(
            layout.size, 16,
            "Enum with u64 payload should have size 16 (8 discr + 8 payload)"
        );
        assert_eq!(layout.alignment, 8, "Enum should have alignment 8");
    }

    #[test]
    fn enum_layout_record_payload() {
        // enum { Pair { a: u8, b: u8 } } — discriminant + record payload
        let mut interner = TypeInterner::new();
        let u8_id = interner.uint(8);

        let enum_id = interner.intern(Type::Enum {
            variants: smallvec::smallvec![(
                1,
                EnumPayload::Record(smallvec::smallvec![(1, u8_id), (2, u8_id)])
            )],
        });

        let layout = layout_of(&interner, enum_id);
        // Layout: 8 bytes discriminant + 2 bytes payload (u8+u8) + 6 bytes padding = 16 bytes
        assert_eq!(
            layout.size, 16,
            "Enum with record payload should be 16 bytes (8 discr + 2 payload + 6 pad)"
        );
        assert_eq!(layout.alignment, 8, "Enum should have alignment 8");
    }

    #[test]
    fn enum_layout_mixed() {
        // enum { Unit, Tuple(u64), Record { x: u8, y: u8 } }
        let mut interner = TypeInterner::new();
        let u64_id = interner.uint(64);
        let u8_id = interner.uint(8);

        let enum_id = interner.intern(Type::Enum {
            variants: smallvec::smallvec![
                (1, EnumPayload::Unit),
                (2, EnumPayload::Tuple(smallvec::smallvec![u64_id])),
                (
                    3,
                    EnumPayload::Record(smallvec::smallvec![(1, u8_id), (2, u8_id)])
                ),
            ],
        });

        let layout = layout_of(&interner, enum_id);
        // Max payload: u64 = 8 bytes
        // Layout: 8 bytes discriminant + 8 bytes payload = 16 bytes, align 8
        assert_eq!(
            layout.size, 16,
            "Mixed enum should be 16 bytes (discriminant + max payload)"
        );
        assert_eq!(layout.alignment, 8, "Mixed enum should have alignment 8");
    }
}
