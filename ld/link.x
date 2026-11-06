/* Linker script for the NXT's AT91SAM7S256.
 *
 * Memory map confirmed against the AT91SAM7S256 datasheet:
 *   - Internal flash: 256 KiB, mapped at 0x0010_0000, and aliased at
 *     0x0000_0000 at reset (GPNVM boot-from-flash is the factory default),
 *     which is why the vector table at address 0 lands in flash with no
 *     extra remap step required.
 *   - Internal SRAM: 64 KiB at 0x0020_0000.
 */

MEMORY
{
    FLASH : ORIGIN = 0x00100000, LENGTH = 256K
    RAM   : ORIGIN = 0x00200000, LENGTH = 64K
}

/* Stacks for each ARM exception mode, carved out of the top of RAM.
 * ARM7TDMI has no NVIC/automatic stacking: each mode (SVC/IRQ/FIQ) banks
 * its own SP, and nxt-rt's reset handler sets each of these explicitly
 * before ever switching into that mode. SVC is the main execution stack;
 * IRQ/FIQ are small since nothing uses them yet (Milestone 0/1 run with
 * IRQ/FIQ masked).
 */
_fiq_stack_size = 256;
_irq_stack_size = 512;
_svc_stack_size = 4K;

ENTRY(_reset);

SECTIONS
{
    .vectors :
    {
        KEEP(*(.vectors))
    } > FLASH

    .text :
    {
        *(.text .text.*)
    } > FLASH

    .rodata :
    {
        *(.rodata .rodata.*)
        . = ALIGN(4);
    } > FLASH

    .data : ALIGN(4)
    {
        _sdata = .;
        *(.data .data.*)
        /* Code that must run from RAM (flash.rs: ram_command). */
        *(.ramfunc .ramfunc.*)
        . = ALIGN(4);
        _edata = .;
    } > RAM AT> FLASH
    _sidata = LOADADDR(.data);

    .bss (NOLOAD) :
    {
        _sbss = .;
        *(.bss .bss.*)
        *(COMMON)
        _ebss = .;
    } > RAM

    /* Banked stacks, growing down from the top of RAM. */
    .stacks (NOLOAD) :
    {
        . = ALIGN(8);
        _fiq_stack_bottom = .;
        . += _fiq_stack_size;
        _fiq_stack_top = .;

        . = ALIGN(8);
        _irq_stack_bottom = .;
        . += _irq_stack_size;
        _irq_stack_top = .;

        . = ALIGN(8);
        _svc_stack_bottom = .;
        . += _svc_stack_size;
        _svc_stack_top = .;
    } > RAM

    /DISCARD/ :
    {
        *(.ARM.exidx*)
        *(.comment)
        *(.eh_frame*)
    }
}

ASSERT(_svc_stack_top <= (0x00200000 + 64K), "stacks overflow RAM");
