//! Interrupt Descriptor Table (IDT)
//!
//! 割り込みハンドラを設定します。

use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use x86_64::VirtAddr;
use crate::arch::x86_64::gdt;
use crate::arch::Cpu;
use lazy_static::lazy_static;

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        unsafe {
            idt.double_fault.set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt.page_fault.set_handler_fn(page_fault_handler);
        // Timer Interrupt (IRQ0 -> 32)
        idt[32].set_handler_fn(timer_interrupt_handler);
        // Keyboard Interrupt (IRQ1 -> 33)
        idt[33].set_handler_fn(keyboard_interrupt_handler);
        idt
    };
}

/// IDT を初期化
pub fn init_idt() {
    IDT.load();
}

extern "x86-interrupt" fn breakpoint_handler(_stack_frame: InterruptStackFrame) {
    use crate::arch::x86_64::port::PortWriteOnly;
    unsafe {
        let mut serial = PortWriteOnly::<u8>::new(0x3F8);
        for byte in b"[EXCEPTION] BREAKPOINT\n" {
            serial.write(*byte);
        }
    }
}

extern "x86-interrupt" fn double_fault_handler(
    _stack_frame: InterruptStackFrame, _error_code: u64) -> !
{
    use crate::arch::{Cpu, ArchCpu};
    use crate::arch::x86_64::port::PortWriteOnly;
    
    ArchCpu::disable_interrupts();
    
    // シリアル出力
    unsafe {
        let mut serial = PortWriteOnly::<u8>::new(0x3F8);
        for byte in b"[EXCEPTION] DOUBLE FAULT\n" {
            serial.write(*byte);
        }
    }
    
    loop {
        ArchCpu::halt();
    }
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;
    use crate::kernel::mm::page_fault::{is_user_space_address, handle_user_page_fault};
    use crate::kernel::process::PROCESS_TABLE;
    use crate::kernel::mm::allocator::BOOT_INFO_ALLOCATOR;
    use crate::kernel::mm::PHYS_MEM_OFFSET;
    use x86_64::structures::paging::OffsetPageTable;
    
    // Get the faulting address from CR2
    let fault_addr = Cr2::read().unwrap_or(VirtAddr::new(0));
    
    // Check if this is a user-space page fault
    if is_user_space_address(fault_addr) {
        crate::debug_println!(
            "[PageFault] User space fault at {:#x}, error: {:?}",
            fault_addr.as_u64(),
            error_code
        );
        
        // Try to handle the user-space page fault
        let handled = (|| -> Result<(), ()> {
            // Get current process
            let mut table = PROCESS_TABLE.lock();
            let process = table.current_process_mut().ok_or(())?;
            
            // Get frame allocator and physical memory offset
            let mut allocator_lock = BOOT_INFO_ALLOCATOR.lock();
            let frame_allocator = allocator_lock.as_mut().ok_or(())?;
            let phys_mem_offset = VirtAddr::new(PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed));
            
            // Create mapper for the process's page table
            let l4_table_ptr = (phys_mem_offset + process.page_table_frame().start_address().as_u64()).as_mut_ptr();
            let l4_table = unsafe { &mut *l4_table_ptr };
            let mut mapper = unsafe { OffsetPageTable::new(l4_table, phys_mem_offset) };
            
            // Handle the page fault
            handle_user_page_fault(fault_addr, error_code, &mut mapper, frame_allocator)
                .map_err(|e| {
                    crate::debug_println!("[PageFault] Failed to handle: {:?}", e);
                    ()
                })
        })();
        
        if handled.is_ok() {
            crate::debug_println!("[PageFault] User-space page fault handled successfully");
            return; // Successfully handled, return to user space
        }
        
        crate::debug_println!("[PageFault] Failed to handle user-space page fault, terminating process");
        // Fall through to panic
    }
    
    // Kernel page fault or unhandled user fault - panic
    use crate::arch::x86_64::port::PortWriteOnly;
    
    unsafe {
        let mut serial = PortWriteOnly::<u8>::new(0x3F8);
        for byte in b"[EXCEPTION] PAGE FAULT\n" {
            serial.write(*byte);
        }
        
        // Print fault address
        for byte in b"Fault address: " {
            serial.write(*byte);
        }
        for byte in alloc::format!("{:#x}\n", fault_addr.as_u64()).bytes() {
            serial.write(byte);
        }
        
        // Print error code
        for byte in b"Error code: " {
            serial.write(*byte);
        }
        for byte in alloc::format!("{:?}\n", error_code).bytes() {
            serial.write(byte);
        }
        
        // Print instruction pointer
        for byte in b"Instruction pointer: " {
            serial.write(*byte);
        }
        for byte in alloc::format!("{:#x}\n", stack_frame.instruction_pointer.as_u64()).bytes() {
            serial.write(byte);
        }
    }
    
    loop {
        crate::arch::ArchCpu::halt();
    }
}

#[allow(clippy::missing_const_for_fn)]
extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // 何もしない - EOIさえ送らずリターン
}

extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use crate::arch::x86_64::pic::PICS;
    use crate::kernel::driver::keyboard::{KEYBOARD, SCANCODE_QUEUE};

    // キーボードからスキャンコードを読み取る
    let scancode = KEYBOARD.lock().read_scancode();

    if let Some(scancode) = scancode {
        // キューに追加（Waker もここで呼ばれる）
        SCANCODE_QUEUE.lock().add_scancode(scancode);
    }

    unsafe {
        PICS.lock().notify_end_of_interrupt(33);
    }
}
