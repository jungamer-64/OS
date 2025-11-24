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
    
    ; Save arguments
    ; mov r10, rdx      ; CR3 (NOT USED - Phase 3: postponed to Phase 4)
    mov r11, rsi      ; User stack
    mov r12, rcx      ; RFLAGS
    mov r13, rdi      ; Entry point
    
    ; Build iretq frame on KERNEL stack
    mov rax, 0x23
    push rax          ; SS (user data)
    push r11          ; RSP (user stack)
    push r12          ; RFLAGS
    mov rax, 0x1b
    push rax          ; CS (user code)
    push r13          ; RIP (entry point)
    
    ; [PHASE 3] CR3 switching postponed to Phase 4
    ; Instead, user code is mapped to kernel page table
    ; mov cr3, r10
    
    ; iretq will load SS:RSP, RFLAGS, CS:RIP from stack
    iretq
