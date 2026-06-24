/* Minimal stub partner for PA7C R1.5/R2.5 linking test.
 * Provides entry point and minimal runtime stubs.
 */

.text
.globl _start
_start:
    ret

.globl uart_init
uart_init:
    ret

.globl uart_puts
uart_puts:
    ret

.globl add_one
add_one:
    ret

.globl banner_msg
banner_msg: .quad 0

.globl cap_smoke
cap_smoke: ret

.globl ipc_smoke
ipc_smoke: ret
