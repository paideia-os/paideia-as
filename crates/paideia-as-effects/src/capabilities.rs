//! Built-in capabilities registry.
//!
//! Canonical built-in capabilities are registered here so user code can
//! reference them directly in capability-set expressions (e.g., `@{paideia.raw_mem}`).
//!
//! Per phase-3-m1-005: `paideia.raw_mem` is the built-in capability
//! associated with raw pointer dereferences. Paired with the `RawMem`
//! effect declared in `src/toolchain/abi/abi.pdx`.

/// Built-in capabilities list.
///
/// Each entry is a dotted capability name that can be used in user code
/// without explicit declaration.
pub const BUILTIN_CAPABILITIES: &[&str] = &["paideia.raw_mem"];

/// Check whether a capability name is a built-in capability.
///
/// # Arguments
///
/// * `name` - The capability name to check (e.g., `"paideia.raw_mem"`).
///
/// # Returns
///
/// `true` if the name matches a built-in capability, `false` otherwise.
#[must_use]
pub fn is_builtin_capability(name: &str) -> bool {
    BUILTIN_CAPABILITIES.contains(&name)
}

/// Look up the index of a built-in capability by name.
///
/// Returns `Some(index)` if the capability is built-in, `None` otherwise.
#[must_use]
pub fn builtin_capability_index(name: &str) -> Option<usize> {
    BUILTIN_CAPABILITIES.iter().position(|&cap| cap == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_builtin_capability_accepts_paideia_raw_mem() {
        assert!(is_builtin_capability("paideia.raw_mem"));
    }

    #[test]
    fn is_builtin_capability_rejects_unknown() {
        assert!(!is_builtin_capability("paideia.unknown"));
        assert!(!is_builtin_capability("io.read_cap"));
        assert!(!is_builtin_capability(""));
    }

    #[test]
    fn builtin_capability_index_returns_correct_position() {
        let idx = builtin_capability_index("paideia.raw_mem").expect("should find index");
        assert_eq!(idx, 0);
    }

    #[test]
    fn builtin_capability_index_returns_none_for_unknown() {
        assert!(builtin_capability_index("unknown").is_none());
    }

    #[test]
    fn builtin_capabilities_list_not_empty() {
        assert!(!BUILTIN_CAPABILITIES.is_empty());
    }

    #[test]
    fn builtin_capabilities_contains_paideia_raw_mem() {
        assert!(BUILTIN_CAPABILITIES.contains(&"paideia.raw_mem"));
    }
}
