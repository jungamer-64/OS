//! シンプルなコマンドラインシェル
//!
//! キーボード入力を受け取り、コマンドを実行します。

use crate::kernel::driver::keyboard::ScancodeStream;
use crate::println;
use crate::print;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use alloc::string::String;

// セキュリティ関連の定数
/// 最大入力長（バッファオーバーフロー防止）
const MAX_INPUT_LENGTH: usize = 1024;
/// 最大コマンド長
const MAX_COMMAND_LENGTH: usize = 256;
/// 最大メモリ割り当てサイズ（1MB）
const MAX_ALLOC_SIZE: usize = 1024 * 1024;

pub async fn run() {
    // let mut scancode_stream = ScancodeStream::new();
    let mut keyboard = Keyboard::new(ScancodeSet1::new(), layouts::Us104Key, HandleControl::Ignore);
    let mut line_buffer = String::new();

    println!("Welcome to Tiny OS Shell!");
    print!("> ");

    loop {
        let scancode = ScancodeStream::new().await;

        if let Ok(Some(key_event)) = keyboard.add_byte(scancode)
            && let Some(key) = keyboard.process_keyevent(key_event) {
                match key {
                    DecodedKey::Unicode(character) => {
                        match character {
                            '\n' => {
                                println!();
                                execute_command(&line_buffer);
                                line_buffer.clear();
                                print!("> ");
                            }
                            '\x08' => { // Backspace
                                if !line_buffer.is_empty() {
                                    line_buffer.pop();
                                    // バックスペースの表示処理 (カーソルを戻して空白で上書きして戻す)
                                    print!("\x08 \x08");
                                }
                            }
                            c => {
                                // 入力長制限チェック
                                if line_buffer.len() >= MAX_INPUT_LENGTH {
                                    println!();
                                    println!("[WARNING] Input too long (max {} chars)", MAX_INPUT_LENGTH);
                                    line_buffer.clear();
                                    print!("> ");
                                } else {
                                    print!("{}", c);
                                    line_buffer.push(c);
                                }
                            }
                        }
                    }
                    DecodedKey::RawKey(_key) => {
                        // 特殊キーの処理 (必要に応じて実装)
                        // println!("RawKey: {:?}", key);
                    }
                }
            }
    }
}

fn execute_command(command: &str) {
    use crate::kernel::driver::framebuffer::Color;
    let trimmed = command.trim();
    match trimmed {
        "help" => {
            println!("Available commands:");
            println!("  help  - Show this help message");
            println!("  echo  - Echo the input");
            println!("  clear - Clear the screen");
            println!("  rect  - Draw a test rectangle");
            println!("  circle - Draw a test circle");
            println!("  info   - Show kernel info");
            println!("  sysinfo- Show system information");
            println!("  meminfo- Show memory statistics");
            println!("  alloc  - Test memory allocation (alloc <size>)");
            println!("  cls    - Alias for clear");
        }
        "clear" | "cls" => {
            if let Some(fb) = crate::kernel::driver::framebuffer::FRAMEBUFFER.get() {
                fb.lock().clear();
            }
        }
        "rect" => {
            if let Some(fb) = crate::kernel::driver::framebuffer::FRAMEBUFFER.get() {
                let mut fb = fb.lock();
                fb.draw_rect(100, 100, 100, 100, Color::WHITE);
                println!("Drawn rectangle at (100, 100)");
            }
        }
        "circle" => {
            if let Some(fb) = crate::kernel::driver::framebuffer::FRAMEBUFFER.get() {
                let mut fb = fb.lock();
                fb.draw_circle(200, 200, 50, Color::WHITE);
                println!("Drawn circle at (200, 200)");
            }
        }
        "info" => {
            println!("Tiny OS v0.4.0");
            println!("Running on x86_64 UEFI");
        }
        "meminfo" => {
            show_memory_info();
        }
        "sysinfo" => {
            show_system_info();
        }
        cmd if cmd.starts_with("alloc ") => {
            match cmd[6..].trim().parse::<usize>() {
                Ok(size) => {
                    if let Err(err) = validate_alloc_size(size) {
                        println!("[ERROR] {}", err);
                    } else {
                        test_allocation(size);
                    }
                }
                Err(_) => {
                    println!("[ERROR] Invalid size. Usage: alloc <size>");
                }
            }
        }
        cmd if cmd.starts_with("echo ") => {
            println!("{}", &cmd[5..]);
        }
        "" => {} // Empty command
        _ => {
            println!("Unknown command: '{}'", trimmed);
        }
    }
}

fn show_memory_info() {
    use crate::ALLOCATOR;
    
    let stats = ALLOCATOR.stats();
    
    println!("=== Memory Statistics ===");
    println!("Heap Capacity:       {} bytes", stats.heap_capacity.as_usize());
    println!("Current Usage:       {} bytes", stats.current_usage.as_usize());
    println!("Peak Usage:          {} bytes", stats.peak_usage.as_usize());
    println!("Available:           {} bytes", stats.available().as_usize());
    println!("Usage Rate:          {}%", stats.usage_rate());
    println!("Allocations:         {}", stats.allocation_count);
    println!("Deallocations:       {}", stats.deallocation_count);
    println!("Total Allocated:     {} bytes", stats.total_allocated.as_usize());
    println!("Total Deallocated:   {} bytes", stats.total_deallocated.as_usize());
}

fn test_allocation(size: usize) {
    use alloc::vec::Vec;
    
    println!("Allocating {} bytes...", size);
    
    // Before stats
    let before = crate::ALLOCATOR.stats();
    
    // Allocate
    let _test_vec: Vec<u8> = Vec::with_capacity(size);
    
    // After stats
    let after = crate::ALLOCATOR.stats();
    
    println!("Before: {} bytes used", before.current_usage.as_usize());
    println!("After:  {} bytes used", after.current_usage.as_usize());
    println!("Delta:  {} bytes", 
        after.current_usage.as_usize().saturating_sub(before.current_usage.as_usize()));
    
    // Vec will be dropped here, freeing memory
    drop(_test_vec);
    
    let dropped = crate::ALLOCATOR.stats();
    println!("After drop: {} bytes used", dropped.current_usage.as_usize());
}

fn show_system_info() {
    println!("=== System Information ===");
    println!("OS Name:             Tiny OS");
    println!("Version:             v0.4.0");
    println!("Architecture:        x86_64");
    println!("Boot Mode:           UEFI");
    println!("Build Mode:          Debug");
    
    // Show memory capacity from heap stats
    let stats = crate::ALLOCATOR.stats();
    println!("Heap Capacity:       {} KB", stats.heap_capacity.as_usize() / 1024);
}

/// メモリ割り当てサイズの検証
fn validate_alloc_size(size: usize) -> Result<(), &'static str> {
    if size == 0 {
        return Err("Allocation size must be greater than 0");
    }
    if size > MAX_ALLOC_SIZE {
        return Err("Allocation size exceeds maximum limit (1MB)");
    }
    Ok(())
}

/// 入力長の検証
#[allow(dead_code)]
fn validate_input_length(input: &str) -> Result<(), &'static str> {
    if input.len() > MAX_COMMAND_LENGTH {
        return Err("Command too long");
    }
    Ok(())
}
