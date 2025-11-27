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
;
; CRITICAL: After CR3 switch, we cannot access kernel stack anymore.
; So we build the IRETQ frame on the USER stack before CR3 switch,
; then switch CR3 and RSP together, and execute iretq.
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
; Clobbers: none (saves and restores rax, rdx - but avoids stack if stack_safe=0)
%macro SERIAL_CHAR 1
    push rax
    push rdx
    mov dx, 0x3F8
    mov al, %1
    out dx, al
    pop rdx
    pop rax
%endmacro

; Serial output without using stack (safe after CR3 switch)
%macro SERIAL_CHAR_NOSTACK 1
    mov dx, 0x3F8
    mov al, %1
    out dx, al
%endmacro

jump_to_usermode_asm:
    cli
    
    ; Arguments:
    ;   rdi = entry_point
    ;   rsi = user_stack (RSP for user mode, points to top of mapped stack)
    ;   rdx = user_cr3
    ;   rcx = rflags
    
    SERIAL_CHAR 'A'
    
    ; Save CR3 and rflags values
    mov r8, rdx       ; r8 = user_cr3
    mov r9, rcx       ; r9 = rflags
    mov r10, rdi      ; r10 = entry_point
    mov r11, rsi      ; r11 = user_stack (original, will be updated)
    
    ; Set up data segments (while still in kernel, before CR3 switch)
    ; user_data selector = 0x10 (base) | 0x03 (RPL) = 0x13
    mov ax, 0x13
    mov ds, ax
    mov es, ax
    xor ax, ax
    mov fs, ax
    mov gs, ax
    
    SERIAL_CHAR 'B'
    
    ; Build IRETQ frame on USER stack (currently accessible via kernel page table)
    ; We write to the user stack addresses which are mapped in both page tables
    ; Stack layout for iretq (grows down, so we build from top):
    ;   [RSP+32] SS     = 0x13 (user_data | RPL3)
    ;   [RSP+24] RSP    = user_stack (the final RSP after iretq)
    ;   [RSP+16] RFLAGS = rflags
    ;   [RSP+8]  CS     = 0x1B (user_code | RPL3)
    ;   [RSP+0]  RIP    = entry_point
    
    ; Calculate where to put the frame (5 qwords = 40 bytes below user_stack)
    mov rsi, r11          ; rsi = user_stack top
    sub rsi, 40           ; rsi = address for iretq frame
    
    ; Write IRETQ frame to user stack (in kernel page table, user stack is accessible)
    mov qword [rsi + 32], 0x13   ; SS
    mov [rsi + 24], r11          ; RSP (original user_stack, where we'll end up)
    mov [rsi + 16], r9           ; RFLAGS
    mov qword [rsi + 8], 0x1B    ; CS
    mov [rsi + 0], r10           ; RIP (entry_point)
    
    SERIAL_CHAR 'C'
    
    ; Now switch RSP to point to the iretq frame on user stack
    ; This is safe because user stack is mapped in kernel page table too (PHASE 3)
    mov rsp, rsi
    
    ; Switch CR3 to user page table
    ; After this, we can only access user-mapped memory
    mov cr3, r8
    
    SERIAL_CHAR_NOSTACK 'D'
    
    ; IRETQ will:
    ; - Pop RIP from [RSP+0]   = entry_point
    ; - Pop CS  from [RSP+8]   = 0x1B (user_code | RPL3)  
    ; - Pop RFLAGS from [RSP+16] = rflags
    ; - Pop RSP from [RSP+24] = user_stack (original top)
    ; - Pop SS  from [RSP+32] = 0x13 (user_data | RPL3)
    ; And jump to user mode Ring 3
    iretq
