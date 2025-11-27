/* Linker script for x86_64 higher-half kernel */

ENTRY(_start)

/* Higher-half kernel base address */
KERNEL_BASE = 0xFFFFFFFF80000000;

SECTIONS
{
    /* Start at higher-half address */
    . = KERNEL_BASE;

    /* Code section */
    .text ALIGN(4K) : AT(ADDR(.text) - KERNEL_BASE)
    {
        *(.text.boot)
        *(.text .text.*)
    }

    /* Read-only data */
    .rodata ALIGN(4K) : AT(ADDR(.rodata) - KERNEL_BASE)
    {
        *(.rodata .rodata.*)
    }

    /* Data section */
    .data ALIGN(4K) : AT(ADDR(.data) - KERNEL_BASE)
    {
        *(.data .data.*)
    }

    /* BSS section (uninitialized data) */
    .bss ALIGN(4K) : AT(ADDR(.bss) - KERNEL_BASE)
    {
        *(COMMON)
        *(.bss .bss.*)
    }

    /* Debug sections (not loaded) */
    .debug_info     0 : { *(.debug_info) }
    .debug_abbrev   0 : { *(.debug_abbrev) }
    .debug_line     0 : { *(.debug_line) }
    .debug_str      0 : { *(.debug_str) }
    .debug_ranges   0 : { *(.debug_ranges) }

    /* Discard sections */
    /DISCARD/ :
    {
        *(.eh_frame)
        *(.note.gnu.build-id)
    }
}
