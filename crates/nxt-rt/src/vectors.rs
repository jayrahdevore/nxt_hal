//! ARM7TDMI exception vector table and reset handler.
//!
//! Unlike Cortex-M, ARM7TDMI has no NVIC and no automatic register
//! stacking on exception entry: the 8 vector slots at `0x00`-`0x1C` each
//! hold an *instruction* (not a pointer), and each exception mode has its
//! own banked `SP` that software must set up explicitly before using it.
//! None of this is expressible in safe Rust, so it's written directly in
//! assembly and placed in the `.vectors`/`.text` sections that `link.x`
//! expects.

use core::arch::global_asm;

global_asm!(
    r#"
    .section .vectors, "ax"
    .arm

    .global _reset_vector_table
_reset_vector_table:
    b   _reset
    b   _undefined_handler
    b   _swi_handler
    b   _prefetch_abort_handler
    b   _data_abort_handler
    b   _reserved_handler
    b   _irq_handler
    b   _fiq_handler

    .section .text
    .arm

    .global _reset
_reset:
    @ ARM7TDMI boots into Supervisor mode with IRQ/FIQ already masked, but
    @ set every mode explicitly rather than relying on that, since the
    @ next three blocks each need their own banked SP set before anything
    @ could switch into that mode.

    @ FIQ mode (0x11), IRQ+FIQ masked (0xC0): bank its SP.
    mov r0, #0xD1
    msr cpsr_c, r0
    ldr sp, =_fiq_stack_top

    @ IRQ mode (0x12), IRQ+FIQ masked: bank its SP.
    mov r0, #0xD2
    msr cpsr_c, r0
    ldr sp, =_irq_stack_top

    @ Back to Supervisor mode (0x13) for normal execution; this is the
    @ mode _start_rust and main() run in. IRQ/FIQ stay masked here too
    @ -- Milestone 0/1 are polling-only, nothing enables them yet.
    mov r0, #0xD3
    msr cpsr_c, r0
    ldr sp, =_svc_stack_top

    bl  _start_rust

    @ _start_rust is `-> !`; if it ever returned, loop rather than running
    @ off into undefined memory.
_reset_unreachable:
    b   _reset_unreachable

    .global _undefined_handler
_undefined_handler:
    b   _undefined_handler

    .global _swi_handler
_swi_handler:
    b   _swi_handler

    .global _prefetch_abort_handler
_prefetch_abort_handler:
    b   _prefetch_abort_handler

    .global _data_abort_handler
_data_abort_handler:
    b   _data_abort_handler

    .global _reserved_handler
_reserved_handler:
    b   _reserved_handler

    .global _irq_handler
_irq_handler:
    b   _irq_handler

    .global _fiq_handler
_fiq_handler:
    b   _fiq_handler
    "#
);
