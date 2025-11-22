//! Interrupt Descriptor Table (IDT)
//!
//! 割り込みハンドラを設定します。

use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
// use crate::println;
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

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    use crate::arch::x86_64::port::PortWriteOnly;
    unsafe {
        let mut serial = PortWriteOnly::<u8>::new(0x3F8);
        for byte in b"[EXCEPTION] BREAKPOINT\n" {
            serial.write(*byte);
        }
    }
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame, error_code: u64) -> !
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
    use crate::arch::x86_64::port::PortWriteOnly;
    
    unsafe {
        let mut serial = PortWriteOnly::<u8>::new(0x3F8);
        for byte in b"[EXCEPTION] PAGE FAULT\n" {
            serial.write(*byte);
        }
    }
    
    loop {
        crate::arch::ArchCpu::halt();
    }
}

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
