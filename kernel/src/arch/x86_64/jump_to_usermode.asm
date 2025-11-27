; jump_to_usermode.asm
; NASM assembly for user mode transition using SYSRET
;
; Arguments (System V AMD64 ABI):
; rdi = entry_point (RIP)
; rsi = user_stack (RSP)
; rdx = user_cr3 (CR3)
; rcx = rflags (RFLAGS)
;
; SYSRET behavior (64-bit mode):
; - RCX -> RIP (user entry point)
; - R11 -> RFLAGS
; - CS = STAR[63:48] + 16 = 0x08 + 16 = 0x18 (with RPL=3 -> 0x1B)
; - SS = STAR[63:48] + 8  = 0x08 + 8  = 0x10 (with RPL=3 -> 0x13)
; - Does NOT change RSP (must be set before SYSRET)
; - Does NOT change CR3 (must be set before SYSRET)
;
; GDT Layout (SYSRET-compatible):
;   0x08: kernel_code
;   0x10: user_data (SYSRET SS) -> 0x13 with RPL=3
;   0x18: user_code (SYSRET CS) -> 0x1B with RPL=3
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
    
    ; Save arguments - we need to reorganize for SYSRET
    ; SYSRET expects: RCX = RIP, R11 = RFLAGS
    ; So we need:
    ;   rdi -> RCX (entry_point -> RIP for sysret)
    ;   rsi -> RSP (user_stack)
    ;   rdx -> CR3 (user_cr3)
    ;   rcx -> R11 (rflags -> RFLAGS for sysret)
    
    SERIAL_CHAR 'A'
    
    ; Set up data segments BEFORE CR3 switch (while still accessible)
    ; user_data selector = 0x10 (base) | 0x03 (RPL) = 0x13
    mov ax, 0x13
    mov ds, ax
    mov es, ax
    xor ax, ax
    mov fs, ax
    mov gs, ax
    
    SERIAL_CHAR 'B'
    
    ; Set up for SYSRET:
    ; RCX = entry point (will be loaded into RIP)
    ; R11 = RFLAGS (will be loaded into RFLAGS)  
    ; RSP = user stack (sysret does not change RSP)
    mov r11, rcx      ; RFLAGS -> R11
    mov rcx, rdi      ; entry_point -> RCX
    
    ; Set user stack BEFORE CR3 switch
    ; After CR3 switch, we can't access kernel stack anymore
    mov rsp, rsi
    
    SERIAL_CHAR 'C'
    
    ; Switch to user page table
    ; CR3 switch is the point of no return - after this, we can only
    ; execute the remaining instructions and SYSRET
    mov cr3, rdx
    
    ; Output 'D' without using stack (no push/pop after CR3 switch)
    mov dx, 0x3F8
    mov al, 'D'
    out dx, al
    
    ; SYSRET will:
    ; - Load RIP from RCX (entry_point)
    ; - Load RFLAGS from R11
    ; - Load CS from STAR[63:48] + 16 = 0x08 + 16 = 0x18 (with RPL=3 -> 0x1B)
    ; - Load SS from STAR[63:48] + 8  = 0x08 + 8  = 0x10 (with RPL=3 -> 0x13)
    ; - Switch to Ring 3
    ; RSP was already set to user_stack
    ; CR3 was already set to user_cr3
    o64 sysret
