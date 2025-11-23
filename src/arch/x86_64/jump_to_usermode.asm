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
    ; WORKAROUND for Phase 2.5: Skip CR3 switch
    ; Use kernel CR3 for now (security issue, but allows testing User mode)
    ; TODO Phase 3: Implement proper page table setup
    cli
    
    ; Save arguments to preserved registers
    ; mov r10, rdx      ; CR3 (NOT USED for now)
    mov r11, rsi      ; Stack
    mov r12, rcx      ; RFLAGS
    mov r13, rdi      ; Entry point
    
    ; Push iretq frame
    mov rax, 0x23
    push rax          ; SS (user data)
    push r11          ; RSP (user stack)
    push r12          ; RFLAGS
    mov rax, 0x1b
    push rax          ; CS (user code)
    push r13          ; RIP (entry point)
    
    ; Set user data segments
    mov ax, 0x23
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    
    ; WORKAROUND: Skip CR3 switch for Phase 2.5
    ; mov cr3, r10  ; COMMENTED OUT
    
    ; iretq will transition to user mode
    iretq
    ; 
    ; ; CRITICAL FIX: Push iretq frame BEFORE setting user segments!
    ; ; We're still using kernel stack (RSP), so we need kernel data segment
    ; ; to be active during push operations.
    ; 
    ; ; Push iretq frame (using kernel stack with kernel segments)
    ; mov rax, 0x23
    ; push rax          ; SS (user data)
    ; push r11          ; RSP (user stack)
    ; push r12          ; RFLAGS
    ; mov rax, 0x1b
    ; push rax          ; CS (user code)
    ; push r13          ; RIP (entry point)
    ; 
    ; ; Now it's safe to set user data segments
    ; ; (only affects segment registers, not stack operations)
    ; mov ax, 0x23
    ; mov ds, ax
    ; mov es, ax
    ; mov fs, ax
    ; mov gs, ax
    ; 
    ; ; Switch CR3 before iretq
    ; mov cr3, r10
    ; ; NOP sled for debugging - if GPF happens here, we'll see the RIP
    ; nop
    ; nop
    ; nop
    ; nop
    ; nop
    ; 
    ; ; iretq will:
    ; ; 1. Pop RIP (to r13 value = 0x400000)
    ; ; 2. Pop CS (to 0x1B = user code)
    ; ; 3. Pop RFLAGS (to r12 value)
    ; ; 4. Pop RSP (to r11 value = 0x700000000000)
    ; ; 5. Pop SS (to 0x23 = user data)
    ; ; 6. Switch to CPL=3 (Ring 3)
    ; iretq
