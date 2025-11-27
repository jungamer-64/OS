; jump_to_usermode.asm
; NASM assembly for user mode transition using IRETQ
;
; Arguments (System V AMD64 ABI):
; rdi = entry_point (RIP)
; rsi = user_stack (RSP)
; rdx = user_cr3 (CR3)
; rcx = rflags (RFLAGS)
;
; IRETQ pops from stack (in order): RIP, CS, RFLAGS, RSP, SS
; This allows atomic transition to user mode after CR3 switch.
;
; GDT Layout:
;   0x08: kernel_code
;   0x10: user_data -> 0x13 with RPL=3
;   0x18: user_code -> 0x1B with RPL=3
;   0x20: kernel_data
;   0x28: TSS

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
    
    ; Arguments:
    ;   rdi = entry_point
    ;   rsi = user_stack (RSP for user mode)
    ;   rdx = user_cr3
    ;   rcx = rflags
    
    SERIAL_CHAR 'A'
    
    ; Save CR3 value for later
    mov r8, rdx       ; r8 = user_cr3
    
    ; Set up data segments (while still in kernel)
    ; user_data selector = 0x10 (base) | 0x03 (RPL) = 0x13
    mov ax, 0x13
    mov ds, ax
    mov es, ax
    xor ax, ax
    mov fs, ax
    mov gs, ax
    
    SERIAL_CHAR 'B'
    
    ; Build IRETQ frame on current (kernel) stack
    ; Stack layout for iretq (from top):
    ;   [RSP+32] SS     = 0x13 (user_data | RPL3)
    ;   [RSP+24] RSP    = user_stack
    ;   [RSP+16] RFLAGS = rflags
    ;   [RSP+8]  CS     = 0x1B (user_code | RPL3)
    ;   [RSP+0]  RIP    = entry_point
    
    push qword 0x13        ; SS: user_data | RPL3
    push rsi               ; RSP: user_stack
    push rcx               ; RFLAGS
    push qword 0x1B        ; CS: user_code | RPL3  
    push rdi               ; RIP: entry_point
    
    SERIAL_CHAR 'C'
    
    ; Switch CR3 to user page table
    ; The iretq instruction itself is still in the current (kernel) page table
    ; at this point, which is fine because we're about to execute it
    mov cr3, r8
    
    ; At this point:
    ; - CR3 points to user page table
    ; - The iretq frame is on the kernel stack (which is identity-mapped
    ;   or mapped in the user page table via PHASE 3 workaround)
    ; - iretq will atomically load RIP, CS, RFLAGS, RSP, SS and jump to user mode
    
    ; Output 'D' - after this, iretq will transfer to user mode
    mov dx, 0x3F8
    mov al, 'D'
    out dx, al
    
    iretq
