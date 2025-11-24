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
    mov r10, rdx      ; CR3
    mov r11, rsi      ; User stack
    mov r12, rcx      ; RFLAGS
    mov r13, rdi      ; Entry point
    
    ; Build iretq frame on KERNEL stack (accessible in both page tables)
    mov rax, 0x23
    push rax          ; SS (user data)
    push r11          ; RSP (user stack)
    push r12          ; RFLAGS
    mov rax, 0x1b
    push rax          ; CS (user code)
    push r13          ; RIP (entry point)
    
    ; [PHASE 3] Switch CR3 to user page table
    mov cr3, r10
    
    ; NOP sled to identify where failure occurs
    nop
    nop
    nop
    nop
    nop
    
    ; iretq will:
    ; 1. Set CS to 0x1b (user code) and RIP to entry point
    ; 2. Set RFLAGS
    ; 3. Set SS to 0x23 (user data) and RSP to user stack
    ; Note: DS, ES, FS, GS remain as kernel segments
    ; User code must set them if needed
    iretq
