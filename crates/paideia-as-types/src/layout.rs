//! Memory layout computation for types.
//!
//! Determines the size and alignment of types, accounting for padding
//! and alignment requirements of record fields. Used by the code generator
//! and linker to correctly position data.

use crate::intern::TypeInterner;
use crate::types::{Type, TypeId};

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
}
