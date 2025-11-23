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

jump_to_usermode_asm:
    cli
    
    ; Save arguments to preserved registers
    mov r10, rdx      ; CR3
    mov r11, rsi      ; Stack
    mov r12, rcx      ; RFLAGS
    mov r13, rdi      ; Entry point
    
    ; Set user data segments
    mov ax, 0x23
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    
    ; Push iretq frame
    mov rax, 0x23
    push rax          ; SS
    push r11          ; RSP
    push r12          ; RFLAGS
    mov rax, 0x1b
    push rax          ; CS
    push r13          ; RIP
    
    ; Switch CR3 and iretq
    mov cr3, r10
    iretq
