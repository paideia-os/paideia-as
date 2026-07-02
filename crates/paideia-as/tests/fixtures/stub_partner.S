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

.globl idt_ok_msg
idt_ok_msg: .quad 0

.globl _tick_msg
_tick_msg: .quad 0

.globl apic_enable
apic_enable:
    ret

.globl lapic_timer_init
lapic_timer_init:
    ret

.globl lapic_timer_rearm
lapic_timer_rearm:
    ret

.globl apic_eoi
apic_eoi:
    ret
