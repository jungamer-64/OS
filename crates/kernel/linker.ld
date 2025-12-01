/* linker.ld - x86_64 Higher-Half Kernel Linker Script */

ENTRY(_start)

/* 定数定義 */
KERNEL_BASE = 0xFFFFFFFF80000000;
PAGE_SIZE = 4K;

SECTIONS
{
    /* Higher-half addressing */
    . = KERNEL_BASE;

    /* テキストセクション（実行可能コード） */
    .text ALIGN(PAGE_SIZE) : AT(ADDR(.text) - KERNEL_BASE)
    {
        KEEP(*(.text.boot))      /* ブートコードを先頭に配置 */
        *(.text .text.*)
        
        /* テキストセクションの終端マーカー */
        __text_end = .;
    }

    /* Read-onlyデータ */
    .rodata ALIGN(PAGE_SIZE) : AT(ADDR(.rodata) - KERNEL_BASE)
    {
        *(.rodata .rodata.*)
        __rodata_end = .;
    }

    /* 初期化済みデータ */
    .data ALIGN(PAGE_SIZE) : AT(ADDR(.data) - KERNEL_BASE)
    {
        *(.data .data.*)
        __data_end = .;
    }

    /* 未初期化データ（BSS） */
    .bss ALIGN(PAGE_SIZE) : AT(ADDR(.bss) - KERNEL_BASE)
    {
        __bss_start = .;
        *(COMMON)
        *(.bss .bss.*)
        __bss_end = .;
    }

    /* カーネル終端マーカー */
    __kernel_end = .;

    /* デバッグセクション（メモリにロードしない） */
    .debug_info     0 : { *(.debug_info .debug_info.*) }
    .debug_abbrev   0 : { *(.debug_abbrev .debug_abbrev.*) }
    .debug_line     0 : { *(.debug_line .debug_line.*) }
    .debug_str      0 : { *(.debug_str .debug_str.*) }
    .debug_ranges   0 : { *(.debug_ranges .debug_ranges.*) }
    .debug_loc      0 : { *(.debug_loc .debug_loc.*) }
    .debug_frame    0 : { *(.debug_frame .debug_frame.*) }

    /* 不要なセクションを破棄 */
    /DISCARD/ :
    {
        *(.eh_frame)
        *(.eh_frame_hdr)
        *(.note.gnu.build-id)
        *(.comment)
    }
}
