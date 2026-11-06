/* Linker script for RAM-resident debug probes uploaded over SAM-BA.
 * Image lives at 0x00202000 (clear of the monitor's low-RAM use). */
MEMORY { RAM : ORIGIN = 0x00202000, LENGTH = 16K }
ENTRY(probe)
SECTIONS
{
    .text : { KEEP(*(.text.probe)) *(.text .text.*) } > RAM
    .rodata : { *(.rodata .rodata.*) } > RAM
    /DISCARD/ : { *(.ARM.exidx*) *(.comment) *(.eh_frame*) *(.data .data.* .bss .bss.*) }
}
