//! Mode propagation tests (Phase 15 m2-002).

#[cfg(test)]
mod tests {
    use paideia_as_ir::instruction::InstrMode;

    /// T1: no #![bits] → all Mode64
    #[test]
    fn mode_default_no_bits_attr() {
        // TODO: construct a minimal IR with no #![bits] inner_attr,
        // run EmitWalker, verify all instructions have mode=Mode64.
    }

    /// T2: root #![bits = 32] → all Mode32
    #[test]
    fn mode_propagate_bits_32() {
        // TODO: construct a minimal IR with #![bits=32] inner_attr at root,
        // run EmitWalker, verify all instructions have mode=Mode32.
    }

    /// T3: explicit #![bits = 64] → all Mode64
    #[test]
    fn mode_explicit_bits_64() {
        // TODO: construct a minimal IR with #![bits=64] inner_attr,
        // run EmitWalker, verify all instructions have mode=Mode64.
    }

    /// T4: invalid value (B1700/P0240 at parse) → walker falls back to Mode64
    #[test]
    fn mode_invalid_bits_fallback_mode64() {
        // TODO: test that invalid bits values (non-32/64) are rejected at parse,
        // or if accepted in IR, walker gracefully falls back to Mode64.
    }
}
