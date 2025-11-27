/* Userland linker script for Rany OS */
/* Places all code and data starting at 0x400000 */

ENTRY(_start)

PHDRS
{
    text PT_LOAD FLAGS(5);  /* r-x */
    rodata PT_LOAD FLAGS(4); /* r-- */
    data PT_LOAD FLAGS(6);   /* rw- */
    stack PT_GNU_STACK FLAGS(6);
}

SECTIONS
{
    . = 0x400000;
    
    .text : {
        *(.text.entry)
        *(.text .text.*)
    } :text
    
    /* Align to page boundary to avoid overlapping segments */
    . = ALIGN(0x1000);
    
    .rodata : {
        *(.rodata .rodata.*)
    } :rodata
    
    /* Align to page boundary */
    . = ALIGN(0x1000);
    
    .data : {
        *(.data .data.*)
    } :data
    
    .bss : {
        *(.bss .bss.*)
    } :data
    
    /* Discard unneeded sections */
    /DISCARD/ : {
        *(.comment)
        *(.eh_frame)
        *(.eh_frame_hdr)
    }
}
