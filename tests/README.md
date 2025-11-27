# Tiny OS - Integration Tests

このディレクトリには、Tiny OSのカーネル統合テストが含まれています。

## テスト一覧

### basic_boot.rs

基本的な起動テスト。カーネルが正常に起動し、初期化が完了することを確認します。

### io_synchronization.rs

I/O同期テスト。シリアル出力やVGAバッファの同期処理が正しく動作することを確認します。

### should_panic.rs

パニックハンドラのテスト。意図的にパニックを発生させ、パニックハンドラが正常に動作することを確認します。

### syscall_alignment_test.rs

システムコールのアライメントテスト。syscallインターフェースのメモリアライメントが正しいことを確認します。

### vga_buffer.rs

VGAバッファのテスト。VGAバッファへの書き込みが正常に動作することを確認します。

## テストの実行

```powershell
# 全ての統合テストを実行
cargo test --workspace

# 特定のテストを実行
cargo test --test basic_boot
cargo test --test io_synchronization
cargo test --test should_panic
cargo test --test syscall_alignment_test
cargo test --test vga_buffer
```

## テストの追加

新しい統合テストを追加するには、`tests/integration/`ディレクトリに新しい`.rs`ファイルを作成してください。

```rust
// tests/integration/your_test.rs
#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(tiny_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

use tiny_os::println;
use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    test_main();
    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    tiny_os::test_panic_handler(info)
}

// Add your tests here
#[test_case]
fn your_test() {
    // Your test implementation
}
```

## 注意事項

- 統合テストは`no_std`環境で実行されます
- 各テストは独立したバイナリとしてビルドされます
- QEMUが必要です（`run_qemu.ps1`スクリプトを参照）
