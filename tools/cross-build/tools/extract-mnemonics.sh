#!/bin/bash
# Extract mnemonics and operands from an ELF .o file via objdump.
#
# Takes a .o file as $1. Runs objdump -d, strips address/encoding columns,
# emits one mnemonic+operands per line. Blank lines and comments stripped.
#
# Usage:
#   extract-mnemonics.sh module.o > actual-mnemonics.txt
#
# Requires: binutils (objdump)

set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "Usage: $(basename "$0") <elf-object-file>" >&2
    exit 1
fi

obj_file="$1"

if [[ ! -f "$obj_file" ]]; then
    echo "Error: $obj_file not found" >&2
    exit 1
fi

# objdump -d produces lines like:
#   0000000000000000 <add_one>:
#        0:	48 8d 47 01          	lea    rax,[rdi+0x1]
#        4:	c3                   	ret
#
# We want to extract just "lea    rax,[rdi+0x1]" and "ret".
# Drop the address prefix (up to and including the tab after hex), then
# strip any trailing comments (after #). Keep only mnemonic + operands.

objdump -d -M intel "$obj_file" \
    | awk '
        # Skip lines that do not contain an instruction (function headers, blank lines)
        /^[0-9a-f]+\s+</ { next }
        /^\s*$/ { next }
        /^Disassembly/ { next }

        # Match instruction lines: optional spaces, hex address, colon, optional spaces,
        # hex bytes, then tab, then the mnemonic+operands
        /^[[:space:]]*[0-9a-f]+:/ {
            # Extract everything after the hex bytes and tab
            # The pattern is: address: bytes<tab>mnemonic operands
            # We find the last tab-separated field or use regex to skip the hex part

            # Remove leading/trailing whitespace, then extract from first non-space after the hex part
            sub(/^[[:space:]]*[0-9a-f]+:[[:space:]]*[0-9a-f[:space:]]*/, "")

            # Remove any inline comments (after #)
            sub(/#.*/, "")

            # Trim trailing whitespace
            sub(/[[:space:]]+$/, "")

            # Only print non-empty lines
            if (NF > 0) {
                print
            }
        }
    '
