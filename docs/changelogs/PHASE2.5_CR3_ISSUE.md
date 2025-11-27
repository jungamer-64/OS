# Phase 2.5 CR3 Switching Issue

## Summary

User mode transition works **without CR3 switch**, but fails with **Double Fault (GPF 0xd)** when switching to User CR3 before `iretq`.

## Current Status (2025-11-24)

- ✅ User page table creation (copying all kernel mappings)
- ✅ `iretq` instruction works (transitions to Ring 3)
- ❌ **CR3 switching causes Double Fault**

## What Works

1. **Without CR3 Switch**:
   - `iretq` successfully transitions to User mode (CPL=3)
   - User code entry point (0x400000) is reached
   - Page fault at user code (expected - not mapped in kernel CR3)

2. **User Page Table Setup**:
   - All kernel L4 entries (0-6, 511) copied to User page table
   - Verification confirms 7 entries present
   - No USER_ACCESSIBLE flag on kernel pages (secure)

## What Fails

**CR3 Switch + iretq = Double Fault**:

- Switching to User CR3 (0x538000) before `iretq`
- Double Fault occurs immediately
- QEMU log shows: `check_exception old: 0xffffffff new 0xd` (GPF)
- Then: `check_exception old: 0xd new 0xb` (Double Fault)

## Investigation Results

### Test 1: CR3 Switch Alone

```asm
mov cr3, r10  ; Switch to User CR3
hlt           ; Halt
```

**Result**: No Double Fault, but CPU doesn't reach HLT loop (RIP stuck in UEFI code)

### Test 2: CR3 Switch + iretq (No Segment Setup)

```asm
; Push iretq frame
mov cr3, r10
iretq
```

**Result**: Double Fault (GPF 0xd)

### Test 3: No CR3 Switch + Full iretq

```asm
; Push iretq frame
; Set DS/ES/FS/GS to 0x23
iretq  ; CR3 NOT switched
```

**Result**: Success! Transitions to User mode, then Page Fault at 0x400000 (expected)

## Root Cause Hypothesis

The issue is **NOT** with:

- ❌ iretq stack frame (verified correct)
- ❌ GDT setup (verified correct: 0x08, 0x10, 0x18, 0x20)
- ❌ User page table entries (verified: all 7 kernel entries present)
- ❌ Page table flags (verified: kernel pages have no USER_ACCESSIBLE)

**Suspected cause**:
The UEFI bootloader's initial page table structure is **not compatible** with User CR3 switching during `iretq`. Possible reasons:

1. Some critical kernel structures (GDT, IDT, or kernel code itself) are not properly mapped in User CR3's address space hierarchy
2. The `iretq` instruction may access intermediate page table levels (PDPT, PD, PT) that don't exist in User CR3
3. TLB flushing after `mov cr3` may not complete before `iretq` executes

## Workaround (Phase 2.5)

**Use kernel CR3 for all processes**:

- Skip `mov cr3, r10` in `jump_to_usermode_asm`
- All processes share kernel page table
- **Security implication**: No memory isolation between processes
- **Acceptable for Phase 2.5** testing (User mode, syscalls, scheduler)

## Code Changes

`src/arch/x86_64/jump_to_usermode.asm`:

```asm
; WORKAROUND: Skip CR3 switch for Phase 2.5
; mov cr3, r10  ; COMMENTED OUT
```

## Future Work (Phase 3)

**Proper fix requires**:

1. **Rebuild kernel page table from scratch**
   - Don't rely on UEFI bootloader's page table
   - Create kernel page table with User mode transition in mind

2. **Implement recursive mapping or temporary mapping**
   - Allow kernel to access User page tables
   - Ensure all page table levels are accessible

3. **Test with minimal page table**
   - Start with only essential mappings
   - Gradually add complexity

4. **Study Linux/xv6 implementation**
   - How do they handle User CR3 switching?
   - What page table structure do they use?

## References

- Intel SDM Vol. 3A, Section 6.14.2: Double Faults
- Intel SDM Vol. 3A, Section 4.5: Paging
- QEMU log: `check_exception` shows GPF (0xd) → Double Fault (0xb)
- Test results: CR3 switch alone doesn't cause immediate fault, but `iretq` after CR3 switch does

## Next Steps

1. ✅ Complete Phase 2.5 with kernel CR3 workaround
2. ⏳ Test User mode execution with shared address space
3. ⏳ Test syscall mechanism
4. ⏳ Test basic scheduler operation
5. ⏳ **Phase 3**: Redesign page table structure for proper CR3 isolation
