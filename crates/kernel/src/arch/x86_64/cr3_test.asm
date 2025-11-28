; crates/kernel/src/arch/x86_64/cr3_test.asm
; NASM assembly for testing CR3 switch in isolation
; This helps diagnose the CR3 switching issue

BITS 64

global test_cr3_switch

; Test CR3 switch without iretq
; Arguments:
; rdi = new_cr3
; Returns: 0 on success
test_cr3_switch:
    cli
    
    ; Save current CR3
    mov rax, cr3
    push rax
    
    ; Load new CR3
    mov cr3, rdi
    
    ; If we reach here, CR3 switch succeeded
    ; Do some NOPs to ensure CPU continues
    nop
    nop
    nop
    nop
    
    ; Restore original CR3
    pop rax
    mov cr3, rax
    
    ; Return success
    xor rax, rax
    ret

; Test iretq without CR3 switch
global test_iretq_only

test_iretq_only:
    cli
    
    ; Save current stack pointer
    mov rbx, rsp
    
    ; Push iretq frame (stay in kernel mode)
    ; Order (bottom to top): SS, RSP, RFLAGS, CS, RIP
    mov rax, ss
    push rax              ; SS (kernel data)
    push rbx              ; RSP (current stack)
    pushf                 ; RFLAGS
    mov rax, cs
    push rax              ; CS (kernel code)
    lea rax, [rel .return_point]
    push rax              ; RIP
    
    iretq
    
.return_point:
    ; If we reach here, iretq succeeded
    xor rax, rax
    ret

; Test CR3 switch + simple code execution
global test_cr3_with_execution

test_cr3_with_execution:
    cli
    
    ; Switch CR3
    mov cr3, rdi
    
    ; Try to execute some code
    mov rax, 0x12345678
    mov rbx, rax
    xor rax, rax
    
    ret
