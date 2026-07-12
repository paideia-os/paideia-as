//! Runtime execution tests for compiled Paideia fixtures.
//!
//! This harness:
//! 1. Compiles a .pdx fixture to a .o object file
//! 2. Synthesizes a C driver that poisons CPU registers
//! 3. Links driver.c + fixture.o with gcc
//! 4. Runs the binary and verifies the exit code

mod common;

mod runtime {
    pub mod harness;
    pub mod driver_template;

    mod match_enum_pattern;
    mod match_nested_pattern;
}
