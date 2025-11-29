ENTRY(_start)

PHDRS {
    text PT_LOAD FLAGS(5);    /* R+X */
    rodata PT_LOAD FLAGS(4);  /* R */
    data PT_LOAD FLAGS(6);    /* R+W */
}

SECTIONS {
    . = 0x8000000000;

    .text : ALIGN(0x1000) {
        KEEP(*(.text._start))
        *(.text.entry)
        *(.text .text.*)
    } :text

    .rodata : ALIGN(0x1000) {
        *(.rodata .rodata.*)
    } :rodata

    .data : ALIGN(0x1000) {
        *(.data .data.*)
    } :data

    .bss : ALIGN(0x1000) {
        *(.bss .bss.*)
        *(COMMON)
    } :data
    
    /DISCARD/ : {
        *(.eh_frame)
        *(.eh_frame_hdr)
        *(.comment)
        *(.note*)
        *(.dynamic)
        *(.dynsym)
        *(.dynstr)
        *(.hash)
        *(.gnu.hash)
        *(.rela.*)
        *(.got*)
        *(.plt*)
    }
}
