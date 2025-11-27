; jump_to_usermode.asm
; NASM assembly for user mode transition
;
; Arguments (System V AMD64 ABI):
; rdi = entry_point (RIP)
; rsi = user_stack (RSP)
; rdx = user_cr3 (CR3)
; rcx = rflags (RFLAGS)

BITS 64

global jump_to_usermode_asm

; Serial port output for debugging (port 0x3F8)
; Clobbers: none (saves and restores rax, rdx)
%macro SERIAL_CHAR 1
    push rax
    push rdx
    mov dx, 0x3F8
    mov al, %1
    out dx, al
    pop rdx
    pop rax
%endmacro

jump_to_usermode_asm:
    cli
    
    ; Save arguments FIRST (before any stack operations)
    mov r11, rsi      ; User stack
    mov r12, rcx      ; RFLAGS
    mov r13, rdi      ; Entry point
    mov r14, rdx      ; User CR3 (save for later use)
    
    ; CRITICAL: Ensure SS is set to kernel data selector (0x10)
    mov ax, 0x10
    mov ss, ax
    
    ; [PHASE 4 FIX] Set DS, ES, FS, GS to user data selector BEFORE iretq
    ; These are not automatically set by iretq and must be correct for Ring 3
    mov ax, 0x23      ; User data selector
    mov ds, ax
    mov es, ax
    xor ax, ax        ; Clear FS/GS (or set to 0x23 if needed)
    mov fs, ax
    mov gs, ax
    
    ; Debug marker 1
    SERIAL_CHAR 'J'
    SERIAL_CHAR '1'
    
    ; Build iretq frame on KERNEL stack
    ; Stack layout after pushes:
    ;   [RSP+32]: SS  = 0x23
    ;   [RSP+24]: RSP = user_stack
    ;   [RSP+16]: RFLAGS = 0x202
    ;   [RSP+8]:  CS  = 0x1B
    ;   [RSP+0]:  RIP = entry_point
    mov rax, 0x23
    push rax          ; SS (user data selector)
    push r11          ; RSP (user stack)
    push r12          ; RFLAGS
    mov rax, 0x1B
    push rax          ; CS (user code selector)
    push r13          ; RIP (entry point)
    
    ; Debug marker 2 - just before iretq
    SERIAL_CHAR 'J'
    SERIAL_CHAR '2'
    
    ; Output current RSP (where iretq frame is)
    ; RSP bits 31:28 -> hex digit
    mov rax, rsp
    shr rax, 28
    and rax, 0xF
    add al, '0'
    cmp al, '9'
    jle .digit1_ok
    add al, 7   ; Convert to A-F
.digit1_ok:
    push rdx
    push rax
    mov dx, 0x3F8
    pop rax
    out dx, al
    pop rdx
    ; RSP bits 27:24
    mov rax, rsp
    shr rax, 24
    and rax, 0xF
    add al, '0'
    cmp al, '9'
    jle .digit2_ok
    add al, 7
.digit2_ok:
    push rdx
    push rax
    mov dx, 0x3F8
    pop rax
    out dx, al
    pop rdx
    
    ; Separator
    SERIAL_CHAR ':'
    
    ; Minimal debug - CR3 switch and immediate iretq test
    ; CR3切り替え前の最終確認
    SERIAL_CHAR 'J'
    SERIAL_CHAR '3'
    
    ; [PHASE 3 CR3 switch enabled] 
    ; Switch to user page table before iretq
    mov cr3, r14
    
    ; Debug marker after CR3 switch (uses stack, but we're still Ring 0)
    SERIAL_CHAR 'C'
    
    ; Final marker immediately before iretq
    SERIAL_CHAR '!'
    
    ; iretq will load SS:RSP, RFLAGS, CS:RIP from stack
    iretq
