# tree-sitter-paideia

A tree-sitter grammar for syntactic parsing of paideia-as source files (`.pdx`).

## Overview

This directory contains a tree-sitter grammar that provides syntactic-only parsing for the paideia-as language. Tree-sitter grammars are consumed by editor plugins (VS Code, Helix, Emacs, Neovim, etc.) for:

- Syntax highlighting
- Code folding
- Bracket matching
- Indentation hinting

## Phase-2-m8-012 Scope

This grammar is a **phase-2-m8-012 minimum viable product**:

1. ✓ Scaffold tree-sitter project (package.json, grammar.js, test corpus structure)
2. ✓ Grammar handles core constructs:
   - Identifiers, literals (numbers, strings, booleans, unit)
   - Keywords (module, structure, functor, let, fn, type, effect, macro, etc.)
   - Comments
   - Function declarations with effect/capability rows
   - Let bindings
   - Type and effect declarations
   - Macro declarations (single-rule and multi-rule forms)
   - Module structures and functors
3. ✓ Test corpus with fixture coverage (basic.txt, functions.txt, modules.txt, macros.txt)
4. **Not included**: CI lane integration (m8-013 or m8-014)

## Structure

```
.
├── grammar.js                 # Tree-sitter grammar definition
├── package.json              # npm package manifest
├── tree-sitter.json          # Tree-sitter configuration
├── queries/
│   └── highlights.scm        # Highlight queries for editors
└── test/
    └── corpus/
        ├── basic.txt         # Literal and primitive expression tests
        ├── functions.txt     # Function declaration tests (5 cases)
        ├── modules.txt       # Module and structure tests (5 cases)
        └── macros.txt        # Macro declaration tests (5 cases)
```

## Local Testing

To test the grammar locally, ensure you have `tree-sitter-cli` installed:

```bash
npm install -g tree-sitter-cli
```

Then, from this directory:

```bash
npm install
tree-sitter generate
tree-sitter test
```

The `tree-sitter test` command runs all test cases in `test/corpus/` against the generated parser and verifies that the parse trees match the expected outputs.

## Compatibility

The grammar tracks the surface syntax of the paideia-as parser but does not attempt semantic precision. Tree-sitter performs syntactic recovery (error recovery for incomplete or malformed input), while the Rust parser in `crates/paideia-as-parser` performs canonical semantic analysis.

This means:

- The tree-sitter grammar will parse a superset of valid paideia-as code.
- The Rust parser's type checking, effect analysis, and linearity verification are not replicated here.
- Future enhancements (e.g., handling macros with complex expansions) may refine the grammar.

## Grammar Coverage

### Core Constructs

- **Module declarations**: `module Name = structure { ... }` and `functor (Param : Type) = ...`
- **Function declarations**: `let name : type = fn (...) -> ... expression`
- **Let bindings**: `let name : type = expression`
- **Type declarations**: `type Name = Type`
- **Effect declarations**: `effect Name { op op_name : ... }`
- **Macro declarations**: Single-rule and multi-rule patterns
- **Effect rows**: `!{Io, Memory, ...}`
- **Capability sets**: `@{CapName, ...}`

### Expressions

- Literals: numbers (decimal, hex), strings, booleans, unit
- Identifiers and function calls
- Binary and unary operators
- Lambda expressions
- Match expressions
- If/else expressions
- Block expressions
- Handler installation with `with ... handle ...`
- Perform expressions
- Unsafe blocks

### Types

- Primitive types (identifiers)
- Linear/affine type annotations
- Function types with effect/capability rows
- Tuple types
- Parenthesized types

## Notes for Future Phases

- **Phase-2-m8-013**: CI lane integration (running `tree-sitter test` in CI/CD)
- **Phase-2-m8-014**: Milestone closure; may include expanded test corpus or refinements
- No Rust-side Cargo integration needed; tree-sitter is npm-based

## License

MIT, consistent with the paideia-as project.
