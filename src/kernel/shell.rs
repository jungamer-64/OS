//! シンプルなコマンドラインシェル
//!
//! キーボード入力を受け取り、コマンドを実行します。

use crate::kernel::driver::keyboard::ScancodeStream;
use crate::println;
use crate::print;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use alloc::string::String;
// use alloc::vec::Vec;
// use core::fmt::Write;

pub async fn run() {
    // let mut scancode_stream = ScancodeStream::new();
    let mut keyboard = Keyboard::new(ScancodeSet1::new(), layouts::Us104Key, HandleControl::Ignore);
    let mut line_buffer = String::new();

    println!("Welcome to Tiny OS Shell!");
    print!("> ");

    loop {
        let scancode = ScancodeStream::new().await;

        if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
            if let Some(key) = keyboard.process_keyevent(key_event) {
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
                                print!("{}", c);
                                line_buffer.push(c);
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
}

fn execute_command(command: &str) {
    let trimmed = command.trim();
    match trimmed {
        "help" => {
            println!("Available commands:");
            println!("  help  - Show this help message");
            println!("  echo  - Echo the input");
            println!("  clear - Clear the screen");
            println!("  rect  - Draw a test rectangle");
            println!("  info  - Show kernel info");
        }
        "clear" => {
            if let Some(fb) = crate::kernel::driver::framebuffer::FRAMEBUFFER.get() {
                fb.lock().clear();
            }
        }
        "rect" => {
            if let Some(fb) = crate::kernel::driver::framebuffer::FRAMEBUFFER.get() {
                use crate::kernel::driver::framebuffer::Color;
                let mut fb = fb.lock();
                // Draw a white rectangle at (100, 100) with size 100x100
                fb.draw_rect(100, 100, 100, 100, Color::WHITE);
                println!("Drawn rectangle at (100, 100)");
            }
        }
        "info" => {
            println!("Tiny OS v0.4.0");
            println!("Running on x86_64 UEFI");
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
