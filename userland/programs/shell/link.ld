ENTRY(_start)

SECTIONS {
    . = 0x400000;

    .text : {
        *(.text.entry)
        *(.text .text.*)
    }

    .rodata : {
        *(.rodata .rodata.*)
    }

    .data : {
        *(.data .data.*)
    }

    .bss : {
        *(.bss .bss.*)
    }
    
    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
    }
}
