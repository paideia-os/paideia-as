"""gdb pretty-printer + commands for paideia vendor DWARF sections.

Phase-2-m11-002 minimum: just a stub that, when sourced, registers
a paideia subcommand printing the vendor section names.

Usage:
    (gdb) source scripts/gdb/paideia.py
    (gdb) paideia sections

Future PRs (Phase 3): real DIE traversal + pretty-printing of
capability-binding sites + effect rows.
"""

import gdb


class PaideiaSections(gdb.Command):
    """List the paideia vendor DWARF sections present in the current binary."""

    def __init__(self):
        super().__init__("paideia sections", gdb.COMMAND_USER)

    def invoke(self, arg, from_tty):
        try:
            output = gdb.execute("info files", to_string=True)
            for line in output.splitlines():
                if ".debug.paideia." in line:
                    print(line.strip())
        except Exception as e:
            print(f"paideia sections error: {e}")


class PaideiaCommand(gdb.Command):
    """Paideia-specific debugger commands. See: paideia sections."""

    def __init__(self):
        super().__init__("paideia", gdb.COMMAND_USER, gdb.COMPLETE_COMMAND, prefix=True)

    def invoke(self, arg, from_tty):
        print("Usage: paideia sections")


PaideiaCommand()
PaideiaSections()
print("paideia gdb extension loaded")
