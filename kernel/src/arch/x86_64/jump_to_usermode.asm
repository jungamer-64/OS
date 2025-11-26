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
    
    ; Debug: Mark entry point
    SERIAL_CHAR 'N'
    SERIAL_CHAR '1'
    SERIAL_CHAR ':'
    
    ; Save arguments FIRST (before any stack operations)
    ; mov r10, rdx      ; CR3 (NOT USED - Phase 3: postponed to Phase 4)
    mov r11, rsi      ; User stack
    mov r12, rcx      ; RFLAGS
    mov r13, rdi      ; Entry point
    
    SERIAL_CHAR 'N'
    SERIAL_CHAR '2'
    SERIAL_CHAR ':'
    
    ; CRITICAL: Ensure SS is set to kernel data selector (0x10)
    ; Long Mode allows SS=NULL for CPL=0, but we need valid SS for push operations
    mov ax, 0x10
    mov ss, ax
    
    SERIAL_CHAR 'N'
    SERIAL_CHAR '3'
    SERIAL_CHAR ':'
    
    ; Build iretq frame on KERNEL stack
    ; Stack layout after pushes:
    ;   [RSP+32]: SS  = 0x23
    ;   [RSP+24]: RSP = user_stack
    ;   [RSP+16]: RFLAGS = 0x202
    ;   [RSP+8]:  CS  = 0x1B
    ;   [RSP+0]:  RIP = entry_point
    mov rax, 0x23
    push rax          ; SS (user data selector)
    
    SERIAL_CHAR 'N'
    SERIAL_CHAR '4'
    SERIAL_CHAR ':'
    
    push r11          ; RSP (user stack)
    push r12          ; RFLAGS
    mov rax, 0x1B
    push rax          ; CS (user code selector)
    push r13          ; RIP (entry point)
    
    SERIAL_CHAR 'N'
    SERIAL_CHAR '5'
    SERIAL_CHAR ':'
    
    ; NOTE: Do NOT modify DS/ES/FS/GS before iretq!
    ; In Long Mode, when CPL=0, the data segment registers can remain
    ; at kernel selector (0x10) or NULL (0x00). After iretq transitions
    ; to Ring 3, the CPU will use the SS selector loaded from the stack.
    ; 
    ; Modifying DS to a Ring 3 selector while at Ring 0 can cause issues
    ; with subsequent memory accesses through DS (including the stack!).
    ;
    ; SOLUTION: Leave DS/ES/FS/GS unchanged. After iretq, if user code
    ; needs them, it can set them up itself. In Long Mode, DS/ES/FS/GS
    ; are largely ignored for memory addressing anyway (except for TLS).
    
    ; [PHASE 3] CR3 switching postponed to Phase 4
    ; Instead, user code is mapped to kernel page table
    ; mov cr3, r10
    
    ; Debug: About to execute iretq
    SERIAL_CHAR 'N'
    SERIAL_CHAR '6'
    SERIAL_CHAR ':'
    
    ; Debug: Dump stack frame before iretq
    ; [RSP+0] = RIP
    ; [RSP+8] = CS
    ; [RSP+16] = RFLAGS
    ; [RSP+24] = RSP
    ; [RSP+32] = SS
    
    ; Print RSP value
    SERIAL_CHAR 'S'
    SERIAL_CHAR '='
    mov rax, rsp
    ; Print high byte of RSP (simplified - just show it's not 0)
    shr rax, 12
    and al, 0x0F
    add al, '0'
    cmp al, '9'
    jle .print_rsp
    add al, 7
.print_rsp:
    push rax
    push rdx
    mov dx, 0x3F8
    out dx, al
    pop rdx
    pop rax
    
    SERIAL_CHAR ':'
    
    ; Print SS from stack (should be 0x23)
    SERIAL_CHAR 'S'
    SERIAL_CHAR 'S'
    SERIAL_CHAR '='
    mov rax, [rsp+32]
    ; Print low nibble
    and al, 0x0F
    add al, '0'
    cmp al, '9'
    jle .print_ss
    add al, 7
.print_ss:
    push rax
    push rdx
    mov dx, 0x3F8
    out dx, al
    pop rdx
    pop rax
    
    SERIAL_CHAR ':'
    
    ; Print CS from stack (should be 0x1B)  
    SERIAL_CHAR 'C'
    SERIAL_CHAR 'S'
    SERIAL_CHAR '='
    mov rax, [rsp+8]
    ; Print low nibble
    and al, 0x0F
    add al, '0'
    cmp al, '9'
    jle .print_cs_low
    add al, 7
.print_cs_low:
    push rax
    push rdx
    mov dx, 0x3F8
    out dx, al
    pop rdx  
    pop rax
    
    ; Print high nibble of CS
    mov rax, [rsp+8]
    shr al, 4
    and al, 0x0F
    add al, '0'
    cmp al, '9'
    jle .print_cs_high
    add al, 7
.print_cs_high:
    push rax
    push rdx
    mov dx, 0x3F8
    out dx, al
    pop rdx
    pop rax
    
    SERIAL_CHAR ':'
    
    ; Print RIP from stack (should be 0x400000)
    SERIAL_CHAR 'R'
    SERIAL_CHAR 'I'
    SERIAL_CHAR 'P'
    SERIAL_CHAR '='
    mov rax, [rsp]
    shr rax, 20     ; Get bits 23-20 (should show '4' for 0x400000)
    and al, 0x0F
    add al, '0'
    cmp al, '9'
    jle .print_rip
    add al, 7
.print_rip:
    push rax
    push rdx
    mov dx, 0x3F8
    out dx, al
    pop rdx
    pop rax
    
    SERIAL_CHAR ':'
    
    ; NOTE: Do NOT use 'call' here! It would corrupt the iretq stack frame
    ; by pushing a return address onto the stack.
    
    ; iretq will load SS:RSP, RFLAGS, CS:RIP from stack
    ; Stack layout at this point:
    ;   [RSP+0]:  RIP = entry_point (r13)
    ;   [RSP+8]:  CS  = 0x1B
    ;   [RSP+16]: RFLAGS = 0x202
    ;   [RSP+24]: RSP = user_stack (r11)
    ;   [RSP+32]: SS  = 0x23
    iretq
