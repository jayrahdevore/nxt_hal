/* Application image for use behind the bootloader. Lives at IMAGE_BASE
 * (nxt_boot_proto::IMAGE_BASE): the page at 0x00104000 is the app header
 * written by the bootloader's FINISH command. The vector table here is
 * never at address 0 (the bootloader owns that); its first word is a
 * `b _reset`, which is what the header's `entry` points at.
 * Everything else is identical to link.x. */

MEMORY
{
    FLASH : ORIGIN = 0x00104100, LENGTH = 240K - 256
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
