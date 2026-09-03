//! Rust hook for `pdx/gen_index_tree.pdx`.
//!
//! v0.32 M1-001 (issue #1387): generational-index tree — the ABA-safe
//! backing store for `KIND_A11Y_NODE` (row-based subtyping, v0.32
//! M1-002 / #1388, consumes this). See
//! `design/roadmap/g7-a11y-toolkit.md` once the g7 wave lands (module
//! not yet present).
//!
//! # Why declaration-only for now
//!
//! Generic-struct monomorphization end-to-end is deferred (#997c);
//! until then, `gen_index_tree.pdx` publishes the API surface (one
//! generic `GenIndexTree<T>` struct + trait `GenIndexTreeOps` with the
//! 5 required free functions) and this hook asserts the surface with a
//! regex-based smoke-parse. The full pdx pipeline (paideia-as `check`)
//! parses the file in `crates/paideia-as-stdlib/tests/parse_pdx.rs`
//! once that harness picks the fixture up; the smoke-parse here runs
//! in the ordinary `cargo test -p paideia-as-stdlib` path with no
//! dependency on the toolchain binary.
//!
//! # Cross-repo contract
//!
//! `NODE_COUNT` and the .pdx `GEN_INDEX_TREE_NODE_COUNT_MAX` must move
//! together — `pdx_exports_node_count_constant` fails loudly on drift.
//! `API_FN_NAMES` similarly mirrors the trait's five method names; the
//! `pdx_free_function_count_matches_api_fn_count` test guards against
//! silent surface growth.

/// Raw `.pdx` source for downstream tooling and the smoke-parse tests.
pub const PDX_SRC: &str = include_str!("../pdx/gen_index_tree.pdx");

/// Number of free functions declared on the `GenIndexTreeOps` trait.
///
/// The 5 declared operations are `gen_index_tree_alloc`,
/// `gen_index_tree_free`, `gen_index_tree_get`, `gen_index_tree_set`,
/// and `gen_index_tree_children`.
pub const API_FN_COUNT: usize = 5;

/// Names of the 5 free functions on `GenIndexTreeOps`. Kept in a single
/// slice so callers wanting to sanity-check the pdx surface can iterate
/// without re-listing.
pub const API_FN_NAMES: [&str; API_FN_COUNT] = [
    "gen_index_tree_alloc",
    "gen_index_tree_free",
    "gen_index_tree_get",
    "gen_index_tree_set",
    "gen_index_tree_children",
];

/// Bootstrap node capacity — mirrors `GEN_INDEX_TREE_NODE_COUNT_MAX` in
/// `pdx/gen_index_tree.pdx`. Both must move together (the
/// `pdx_exports_node_count_constant` test asserts the equality).
pub const NODE_COUNT: u64 = 4096;

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    #[test]
    fn pdx_source_is_non_empty() {
        assert!(!PDX_SRC.is_empty(), "gen_index_tree.pdx must not be empty");
    }

    #[test]
    fn pdx_declares_gen_index_tree_struct() {
        // Generic struct: `struct GenIndexTree<T> {`.
        let re = Regex::new(r"struct\s+GenIndexTree\s*<\s*T\s*>\s*\{").unwrap();
        assert!(
            re.is_match(PDX_SRC),
            "pdx/gen_index_tree.pdx must declare `struct GenIndexTree<T> {{`"
        );
    }

    #[test]
    fn pdx_declares_gen_handle_struct() {
        let re = Regex::new(r"struct\s+GenHandle\s*\{").unwrap();
        assert!(
            re.is_match(PDX_SRC),
            "pdx/gen_index_tree.pdx must declare `struct GenHandle`"
        );
    }

    #[test]
    fn pdx_declares_gen_index_tree_node_struct() {
        let re = Regex::new(r"struct\s+GenIndexTreeNode\s*\{").unwrap();
        assert!(
            re.is_match(PDX_SRC),
            "pdx/gen_index_tree.pdx must declare `struct GenIndexTreeNode`"
        );
    }

    #[test]
    fn pdx_declares_gen_index_tree_ops_trait() {
        let re = Regex::new(r"trait\s+GenIndexTreeOps\s*\{").unwrap();
        assert!(
            re.is_match(PDX_SRC),
            "pdx/gen_index_tree.pdx must declare `trait GenIndexTreeOps`"
        );
    }

    #[test]
    fn pdx_exports_node_count_constant() {
        // `pub let GEN_INDEX_TREE_NODE_COUNT_MAX : u64 = 4096u64` is
        // the canonical stdlib const-like form (see option_u64.pdx).
        let re = Regex::new(
            r"pub\s+let\s+GEN_INDEX_TREE_NODE_COUNT_MAX\s*:\s*u64\s*=\s*(\d+)u64",
        )
        .unwrap();
        let caps = re
            .captures(PDX_SRC)
            .expect("pdx/gen_index_tree.pdx must publish GEN_INDEX_TREE_NODE_COUNT_MAX");
        let n: u64 = caps[1].parse().expect("node count parses as u64");
        assert_eq!(
            n, NODE_COUNT,
            "Rust NODE_COUNT ({}) must match .pdx GEN_INDEX_TREE_NODE_COUNT_MAX ({})",
            NODE_COUNT, n
        );
    }

    #[test]
    fn pdx_declares_five_free_functions() {
        assert_eq!(API_FN_NAMES.len(), API_FN_COUNT, "API_FN_NAMES length drift");
        for name in API_FN_NAMES.iter() {
            let re = Regex::new(&format!(r"fn\s+{}\s*\(", regex::escape(name))).unwrap();
            assert!(
                re.is_match(PDX_SRC),
                "pdx/gen_index_tree.pdx must declare `fn {}(...)`",
                name
            );
        }
    }

    #[test]
    fn pdx_free_function_count_matches_api_fn_count() {
        // Count every `fn gen_index_tree_*(` occurrence and check we
        // don't quietly grow (or shrink) the free-function API surface
        // out of band.
        let re = Regex::new(r"\bfn\s+gen_index_tree_[A-Za-z0-9_]+\s*\(").unwrap();
        let n = re.find_iter(PDX_SRC).count();
        assert_eq!(
            n, API_FN_COUNT,
            "pdx/gen_index_tree.pdx declares {} `fn gen_index_tree_*` entries; expected {}",
            n, API_FN_COUNT
        );
    }

    #[test]
    fn pdx_smoke_parse_balanced_braces() {
        // Cheapest lexical sanity check: struct/trait braces balance.
        // Full pdx parse lives in tests/parse_pdx.rs behind --ignored.
        let opens = PDX_SRC.matches('{').count();
        let closes = PDX_SRC.matches('}').count();
        assert_eq!(opens, closes, "unbalanced braces in gen_index_tree.pdx");
    }
}
