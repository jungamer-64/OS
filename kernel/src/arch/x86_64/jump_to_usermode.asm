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
    
    SERIAL_CHAR 'A'
    
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
    
    SERIAL_CHAR 'B'
    
    ; Set data segments to user data selector BEFORE iretq
    ; DS, ES are not changed by iretq
    mov ax, 0x23
    mov ds, ax
    mov es, ax
    xor ax, ax        ; Clear FS/GS 
    mov fs, ax
    mov gs, ax
    
    SERIAL_CHAR 'C'
    
    ; Switch CR3 to user page table
    mov cr3, r14
    
    SERIAL_CHAR 'D'
    
    ; iretq will load SS:RSP, RFLAGS, CS:RIP from stack
    iretq
