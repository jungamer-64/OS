# カーネル・ユーザーランド分離設計書

**作成日**: 2025年11月23日  
**対象OS**: Tiny OS (x86_64 UEFI)  
**目的**: Ring 0（カーネル空間）とRing 3（ユーザー空間）の明確な分離

---

## エグゼクティブサマリー

Tiny OSを完全なモノリシックカーネルから、カーネル・ユーザー空間が分離された近代的なOSアーキテクチャに進化させます。

### 主要目標

| 目標 | 説明 | 優先度 |
|------|------|--------|
| **メモリ保護** | ユーザープログラムがカーネルメモリに直接アクセス不可 | 🔴 必須 |
| **特権分離** | Ring 0/Ring 3の明確な分離 | 🔴 必須 |
| **システムコール** | 安全なカーネルサービス呼び出し機構 | 🔴 必須 |
| **プロセス管理** | 独立したプロセス空間の実現 | 🟡 重要 |
| **互換性維持** | 既存カーネルコードの動作保証 | 🟢 推奨 |

### 設計原則

1. **段階的実装**: 各Phaseは独立してテスト可能
2. **標準準拠**: x86_64標準のsyscall/sysret使用
3. **最小実装**: 必要最小限の機能から開始
4. **安全性優先**: パフォーマンスよりも安全性を優先

---

## 現状分析

### 現在のアーキテクチャ

```
┌─────────────────────────────────────────┐
│         Ring 0 (Kernel Mode)            │
│  ┌──────────────────────────────────┐   │
│  │  Memory Manager                  │   │
│  │  Device Drivers                  │   │
│  │  Interrupt Handlers              │   │
│  │  Shell (!!!)                     │   │ ← 問題！
│  │  Async Executor                  │   │
│  └──────────────────────────────────┘   │
└─────────────────────────────────────────┘

Ring 3 (User Mode): 存在しない
```

**問題点**:
- すべてのコードがRing 0で実行（セキュリティリスク）
- メモリ保護なし（バグがカーネルクラッシュに直結）
- 特権レベルが未使用（ハードウェア機能の活用不足）

### GDTの現状（✅ 準備完了）

```rust
// src/arch/x86_64/gdt.rs
pub struct Selectors {
    pub kernel_code: SegmentSelector,  // ✅ 既存
    pub kernel_data: SegmentSelector,  // ✅ 既存
    pub user_code: SegmentSelector,    // ✅ 既存
    pub user_data: SegmentSelector,    // ✅ 既存
    pub tss: SegmentSelector,          // ✅ 既存
}
```

**現状**: GDTは既にRing 3セグメント準備済み！

### 型システムの現状（✅ 準備完了）

```rust
// src/kernel/core/types.rs
pub struct TaskId(pub u64);
impl TaskId {
    pub const KERNEL_START: u64 = 1;    // ✅ カーネルタスク範囲
    pub const USER_START: u64 = 1000;   // ✅ ユーザータスク範囲
    pub fn is_kernel(&self) -> bool;    // ✅ 判定メソッド
    pub fn is_user(&self) -> bool;      // ✅ 判定メソッド
}
```

**現状**: 型安全化プロジェクトで既に準備済み！

---

## 目標アーキテクチャ

### 最終形態

```
┌─────────────────────────────────────────┐
│         Ring 0 (Kernel Mode)            │
│  ┌──────────────────────────────────┐   │
│  │  Memory Manager                  │   │
│  │  Low-Level Drivers (HW Access)   │   │
│  │  Interrupt Handlers              │   │
│  │  System Call Dispatcher          │   │ ← 新規！
│  │  Process Manager                 │   │ ← 新規！
│  │  Scheduler                       │   │
│  └──────────────────────────────────┘   │
└─────────────────────────────────────────┘
          ↕ System Call Interface
┌─────────────────────────────────────────┐
│         Ring 3 (User Mode)              │
│  ┌──────────────────────────────────┐   │
│  │  Shell                           │   │ ← 移行！
│  │  User Programs                   │   │
│  │  High-Level Drivers              │   │
│  └──────────────────────────────────┘   │
└─────────────────────────────────────────┘
```

### メモリレイアウト

```
Virtual Address Space (x86_64 Canonical Addressing)

0x0000_0000_0000_0000 ┌──────────────────────────┐
                      │   User Code              │ Ring 3
                      │   (executable)           │
0x0000_0040_0000_0000 ├──────────────────────────┤
                      │   User Data & Heap       │ Ring 3
                      │   (read/write)           │
0x0000_0080_0000_0000 ├──────────────────────────┤
                      │   User Stack             │ Ring 3
                      │   (grows down)           │
0x0000_7FFF_FFFF_FFFF └──────────────────────────┘
                      │                          │
                      │   Non-Canonical Gap      │ ← CPU enforced
                      │   (invalid addresses)    │
0xFFFF_8000_0000_0000 ┌──────────────────────────┐
                      │   Kernel Code & Data     │ Ring 0
                      │   (shared across procs)  │
0xFFFF_8800_0000_0000 ├──────────────────────────┤
                      │   Kernel Heap            │ Ring 0
                      │                          │
0xFFFF_A000_0000_0000 ├──────────────────────────┤
                      │   MMIO & Devices         │ Ring 0
                      │                          │
0xFFFF_FFFF_FFFF_FFFF └──────────────────────────┘
```

**重要**:
- カーネル空間（上位）は全プロセスで共有
- ユーザー空間（下位）はプロセスごとに分離
- Non-Canonical Gap（中央）は無効アドレス（CPU保証）

---

## Phase 1: システムコール機構

### 概要

x86_64の`syscall`/`sysret`命令を使用した高速システムコール実装。

### 実装ファイル

#### 1. `src/arch/x86_64/syscall.rs` (新規)

**責務**:
- MSR（Model Specific Register）初期化
- システムコールエントリポイント（アセンブリ）
- スタック切り替え（ユーザー→カーネル）
- レジスタ保存/復元

**主要コンポーネント**:

```rust
// MSR定義
const IA32_STAR: u32 = 0xC0000081;   // セグメント設定
const IA32_LSTAR: u32 = 0xC0000082;  // エントリポイント
const IA32_FMASK: u32 = 0xC0000084;  // RFLAGSマスク

// 初期化関数
pub fn init() {
    unsafe {
        // STAR: カーネル/ユーザーセグメント設定
        wrmsr(IA32_STAR, star_value());
        
        // LSTAR: システムコールエントリポイント
        wrmsr(IA32_LSTAR, syscall_entry as u64);
        
        // FMASK: 割り込みフラグをクリア
        wrmsr(IA32_FMASK, 0x200);  // IF bit
    }
}

// システムコールエントリ（アセンブリ）
#[naked]
unsafe extern "C" fn syscall_entry() {
    asm!(
        // 1. カーネルスタックに切り替え
        "swapgs",                    // GS <- カーネルGS
        "mov gs:0, rsp",             // ユーザーRSP保存
        "mov rsp, gs:8",             // カーネルRSP復元
        
        // 2. レジスタ保存
        "push rcx",                  // 戻りアドレス
        "push r11",                  // RFLAGS
        "push rbp",
        "push rdi", "push rsi", "push rdx",
        "push r8", "push r9", "push r10",
        
        // 3. システムコールハンドラ呼び出し
        // RAX = syscall番号
        // RDI, RSI, RDX, R10, R8, R9 = 引数
        "mov rcx, r10",              // 4番目の引数調整
        "call {}",                   // syscall_handler()
        
        // 4. レジスタ復元
        "pop r10", "pop r9", "pop r8",
        "pop rdx", "pop rsi", "pop rdi",
        "pop rbp",
        "pop r11",                   // RFLAGS
        "pop rcx",                   // 戻りアドレス
        
        // 5. ユーザースタックに復帰
        "mov rsp, gs:0",             // ユーザーRSP復元
        "swapgs",
        "sysretq",
        
        sym syscall_handler,
        options(noreturn)
    );
}
```

**レジスタ規約（x86_64 System V ABI準拠）**:

| レジスタ | 用途 | 保存責任 |
|---------|------|---------|
| RAX | システムコール番号 / 戻り値 | Caller |
| RDI | 第1引数 | Caller |
| RSI | 第2引数 | Caller |
| RDX | 第3引数 | Caller |
| R10 | 第4引数（RCX代替） | Caller |
| R8 | 第5引数 | Caller |
| R9 | 第6引数 | Caller |
| RCX | 戻りアドレス（syscall使用） | - |
| R11 | RFLAGS（syscall使用） | - |

#### 2. `src/kernel/syscall/mod.rs` (新規)

**責務**:
- システムコール番号定義
- システムコールハンドラ
- システムコールテーブル

**システムコール番号**:

```rust
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallNumber {
    Read = 0,      // ファイル/デバイスから読み取り
    Write = 1,     // ファイル/デバイスに書き込み
    Exit = 60,     // プロセス終了
    GetPid = 39,   // プロセスID取得
    Alloc = 100,   // メモリ割り当て（brk相当）
    Dealloc = 101, // メモリ解放
}
```

**ハンドラ実装**:

```rust
pub extern "C" fn syscall_handler(
    num: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    match SyscallNumber::try_from(num) {
        Ok(SyscallNumber::Write) => sys_write(arg1, arg2, arg3),
        Ok(SyscallNumber::Read) => sys_read(arg1, arg2, arg3),
        Ok(SyscallNumber::Exit) => sys_exit(arg1 as i32),
        Ok(SyscallNumber::GetPid) => sys_getpid(),
        Ok(SyscallNumber::Alloc) => sys_alloc(arg1, arg2),
        Ok(SyscallNumber::Dealloc) => sys_dealloc(arg1, arg2),
        Err(_) => -ENOSYS,  // 未実装
    }
}
```

### テスト計画

#### 単体テスト（Phase 1）

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test_case]
    fn test_msr_initialization() {
        // MSRが正しく設定されるか
        init();
        let star = unsafe { rdmsr(IA32_STAR) };
        assert_eq!(star >> 32, expected_star_value());
    }
    
    #[test_case]
    fn test_syscall_number_conversion() {
        // システムコール番号の変換
        assert_eq!(
            SyscallNumber::try_from(1).unwrap(),
            SyscallNumber::Write
        );
    }
}
```

#### 統合テスト（QEMUで実行）

```rust
// tests/syscall_basic.rs
#![no_std]
#![no_main]

#[test_case]
fn test_syscall_write() {
    // システムコール経由での書き込み
    let msg = "Hello from Ring 3!\n";
    let result = unsafe {
        syscall3(
            SyscallNumber::Write as u64,
            1,  // stdout
            msg.as_ptr() as u64,
            msg.len() as u64,
        )
    };
    assert!(result >= 0);
}

#[test_case]
fn test_syscall_getpid() {
    // プロセスID取得
    let pid = unsafe { syscall0(SyscallNumber::GetPid as u64) };
    assert!(pid > 0);
}
```

### 成功基準

- ✅ MSRが正しく初期化される
- ✅ `syscall`命令でカーネルモードに遷移
- ✅ レジスタが正しく保存/復元される
- ✅ `sysret`でユーザーモードに復帰
- ✅ システムコールハンドラが正しく呼び出される
- ✅ 戻り値がRAXレジスタに設定される

---

## Phase 2: プロセス管理とメモリ分離

### 概要

独立したプロセス空間とユーザー空間ページテーブルの実装。

### 実装ファイル

#### 1. `src/kernel/process/mod.rs` (新規)

**Process構造体**:

```rust
use crate::kernel::core::{ProcessId, TaskState};
use crate::kernel::mm::paging::PageTable;
use crate::arch::x86_64::registers::RegisterState;

pub struct Process {
    /// プロセスID
    pub pid: ProcessId,
    
    /// 親プロセスID
    pub ppid: ProcessId,
    
    /// プロセス状態
    pub state: TaskState,
    
    /// プロセス名
    pub name: &'static str,
    
    /// ページテーブル（CR3）
    pub page_table: PageTable,
    
    /// カーネルモードスタック
    pub kernel_stack: VirtAddr,
    
    /// ユーザーモードスタック
    pub user_stack: VirtAddr,
    
    /// 保存されたレジスタ状態
    pub registers: RegisterState,
    
    /// ユーザーヒープの開始アドレス
    pub heap_start: VirtAddr,
    
    /// ユーザーヒープの現在の終端（brk）
    pub heap_end: VirtAddr,
}

impl Process {
    /// 新しいプロセスを作成
    pub fn new(
        pid: ProcessId,
        name: &'static str,
        entry_point: VirtAddr,
    ) -> KernelResult<Self> {
        // 1. ページテーブル作成
        let page_table = create_user_page_table()?;
        
        // 2. スタック割り当て
        let kernel_stack = allocate_kernel_stack()?;
        let user_stack = allocate_user_stack()?;
        
        // 3. ヒープ領域初期化
        let heap_start = VirtAddr::new(0x0000_0040_0000_0000);
        let heap_end = heap_start;
        
        // 4. レジスタ初期化
        let registers = RegisterState::new_user(
            entry_point,
            user_stack + STACK_SIZE,
        );
        
        Ok(Self {
            pid,
            ppid: ProcessId::INVALID,
            state: TaskState::Ready,
            name,
            page_table,
            kernel_stack,
            user_stack,
            registers,
            heap_start,
            heap_end,
        })
    }
    
    /// プロセスに切り替え
    pub unsafe fn switch_to(&mut self) {
        // 1. ページテーブル切り替え（CR3更新）
        self.page_table.load();
        
        // 2. カーネルスタック設定（TSSに設定）
        set_kernel_stack(self.kernel_stack);
        
        // 3. レジスタ復元
        self.registers.restore();
        
        // 4. ユーザーモードに遷移（iretq or sysret）
    }
}
```

**RegisterState**:

```rust
#[repr(C)]
pub struct RegisterState {
    // 汎用レジスタ
    pub rax: u64, pub rbx: u64, pub rcx: u64, pub rdx: u64,
    pub rsi: u64, pub rdi: u64, pub rbp: u64, pub rsp: u64,
    pub r8: u64, pub r9: u64, pub r10: u64, pub r11: u64,
    pub r12: u64, pub r13: u64, pub r14: u64, pub r15: u64,
    
    // 制御レジスタ
    pub rip: u64,     // プログラムカウンタ
    pub rflags: u64,  // フラグレジスタ
    pub cs: u16,      // コードセグメント
    pub ss: u16,      // スタックセグメント
}

impl RegisterState {
    pub fn new_user(entry_point: VirtAddr, stack_top: VirtAddr) -> Self {
        let selectors = crate::arch::x86_64::gdt::selectors();
        Self {
            rip: entry_point.as_u64(),
            rsp: stack_top.as_u64(),
            rflags: 0x202,  // IF=1（割り込み有効）
            cs: selectors.user_code.0,
            ss: selectors.user_data.0,
            ..Default::default()
        }
    }
}
```

#### 2. `src/kernel/mm/paging.rs` (拡張)

**ユーザーページテーブル管理**:

```rust
/// ユーザー空間用のページテーブルを作成
pub fn create_user_page_table() -> KernelResult<PageTable> {
    let mut page_table = PageTable::new()?;
    
    // 1. カーネル空間をマッピング（全プロセスで共有）
    // 0xFFFF_8000_0000_0000 - 0xFFFF_FFFF_FFFF_FFFF
    map_kernel_space(&mut page_table)?;
    
    // 2. ユーザー空間は空の状態で返す
    // プロセスごとに必要に応じてマッピング
    
    Ok(page_table)
}

/// ユーザーメモリをマッピング（Ring 3アクセス権）
pub fn map_user_memory(
    page_table: &mut PageTable,
    virt_addr: VirtAddr,
    phys_addr: PhysAddr,
    size: LayoutSize,
    flags: PageTableFlags,
) -> KernelResult<()> {
    let mut flags = flags;
    flags.insert(PageTableFlags::USER_ACCESSIBLE);  // U/S bit
    flags.insert(PageTableFlags::PRESENT);
    
    // ページ単位でマッピング
    let pages = (size.get() + 4095) / 4096;
    for i in 0..pages {
        let virt = virt_addr + (i * 4096);
        let phys = phys_addr + (i * 4096);
        
        page_table.map(virt, phys, flags)?;
    }
    
    Ok(())
}

/// カーネル空間をマッピング（Ring 0のみアクセス可）
fn map_kernel_space(page_table: &mut PageTable) -> KernelResult<()> {
    // カーネルコード/データをマッピング
    // U/S bit = 0（カーネルのみアクセス可）
    let flags = PageTableFlags::PRESENT 
        | PageTableFlags::WRITABLE;
        // USER_ACCESSIBLE は設定しない！
    
    // 既存のカーネルマッピングをコピー
    // ...
    
    Ok(())
}
```

### テスト計画

#### 単体テスト（Phase 2）

```rust
#[test_case]
fn test_process_creation() {
    let process = Process::new(
        ProcessId::new(1),
        "test_process",
        VirtAddr::new(0x1000),
    ).expect("Failed to create process");
    
    assert_eq!(process.pid, ProcessId::new(1));
    assert_eq!(process.state, TaskState::Ready);
}

#[test_case]
fn test_user_page_table_isolation() {
    let pt1 = create_user_page_table().unwrap();
    let pt2 = create_user_page_table().unwrap();
    
    // 異なるページテーブルであることを確認
    assert_ne!(pt1.cr3_value(), pt2.cr3_value());
}
```

#### メモリ分離テスト（QEMUで実行）

```rust
#[test_case]
fn test_kernel_memory_protection() {
    // ユーザー空間からカーネルメモリにアクセス
    let kernel_addr = 0xFFFF_8000_0000_0000u64 as *const u64;
    
    // #PF (Page Fault) が発生することを期待
    let result = unsafe { core::ptr::read_volatile(kernel_addr) };
    // この行には到達しないはず
    panic!("Kernel memory was accessible from user space!");
}
```

### 成功基準

- ✅ プロセス構造体が正しく作成される
- ✅ ユーザー空間ページテーブルが独立している
- ✅ カーネル空間が全プロセスで共有される
- ✅ ユーザー空間からカーネルメモリにアクセスできない（#PF発生）
- ✅ コンテキストスイッチが正常に動作する

---

## Phase 3: ユーザーランドライブラリ

### 概要

ユーザー空間プログラムが使用する標準ライブラリの最小実装。

### 実装ファイル

#### `src/userland/mod.rs` (新規)

**システムコールラッパー**:

```rust
#![no_std]

use core::arch::asm;

/// システムコール番号
#[repr(u64)]
pub enum Syscall {
    Read = 0,
    Write = 1,
    Exit = 60,
    GetPid = 39,
    Alloc = 100,
    Dealloc = 101,
}

/// 引数なしのシステムコール
#[inline(always)]
pub unsafe fn syscall0(num: u64) -> i64 {
    let ret: i64;
    asm!(
        "syscall",
        inlateout("rax") num => ret,
        out("rcx") _,
        out("r11") _,
        options(nostack, preserves_flags)
    );
    ret
}

/// 引数3個のシステムコール
#[inline(always)]
pub unsafe fn syscall3(num: u64, arg1: u64, arg2: u64, arg3: u64) -> i64 {
    let ret: i64;
    asm!(
        "syscall",
        inlateout("rax") num => ret,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        out("rcx") _,
        out("r11") _,
        options(nostack, preserves_flags)
    );
    ret
}

/// 標準出力に書き込み
pub fn print(s: &str) -> Result<usize, i64> {
    let result = unsafe {
        syscall3(
            Syscall::Write as u64,
            1,  // stdout
            s.as_ptr() as u64,
            s.len() as u64,
        )
    };
    
    if result >= 0 {
        Ok(result as usize)
    } else {
        Err(result)
    }
}

/// プロセスID取得
pub fn getpid() -> Result<u64, i64> {
    let result = unsafe { syscall0(Syscall::GetPid as u64) };
    
    if result >= 0 {
        Ok(result as u64)
    } else {
        Err(result)
    }
}

/// プロセス終了
pub fn exit(code: i32) -> ! {
    unsafe {
        syscall1(Syscall::Exit as u64, code as u64);
    }
    unreachable!()
}

/// メモリ割り当て（brk相当）
pub fn alloc(size: usize, align: usize) -> Result<*mut u8, i64> {
    let result = unsafe {
        syscall2(
            Syscall::Alloc as u64,
            size as u64,
            align as u64,
        )
    };
    
    if result >= 0 {
        Ok(result as *mut u8)
    } else {
        Err(result)
    }
}
```

**マクロ定義**:

```rust
/// println!マクロ（ユーザー空間版）
#[macro_export]
macro_rules! println {
    () => ($crate::print("\n"));
    ($($arg:tt)*) => ({
        use core::fmt::Write;
        let mut writer = $crate::StdoutWriter;
        let _ = writeln!(writer, $($arg)*);
    })
}

/// 標準出力ライター
pub struct StdoutWriter;

impl core::fmt::Write for StdoutWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        print(s).map_err(|_| core::fmt::Error)?;
        Ok(())
    }
}
```

### 成功基準

- ✅ システムコールラッパーが正しくアセンブリを生成
- ✅ レジスタ規約に準拠している
- ✅ エラーハンドリングが適切
- ✅ `println!`マクロが動作する

---

## Phase 4: シェルのユーザー空間移行

### 概要

既存のシェル（`kernel/shell.rs`）をユーザー空間プログラムに書き換え。

### 実装手順

#### 1. システムコール変換

**Before (Ring 0)**:
```rust
// kernel/shell.rs
use crate::kernel::driver::console::write_console;

pub fn shell_main() {
    write_console(format_args!("Welcome to Tiny OS Shell\n"));
    // ...
}
```

**After (Ring 3)**:
```rust
// userland/shell.rs
use crate::userland::{println, getpid};

pub fn shell_main() {
    println!("Welcome to Tiny OS Shell");
    println!("Running as PID: {}", getpid().unwrap());
    
    loop {
        print!("> ");
        let cmd = read_line();
        execute_command(&cmd);
    }
}
```

#### 2. カーネルからの起動

```rust
// main.rs
fn kernel_main() -> ! {
    // 既存の初期化...
    
    // システムコール機構初期化
    crate::arch::x86_64::syscall::init();
    
    // プロセス管理初期化
    crate::kernel::process::init();
    
    // シェルをユーザープロセスとして起動
    let shell_process = Process::new(
        ProcessId::new(1),
        "shell",
        shell_entry_point,
    ).expect("Failed to create shell process");
    
    // スケジューラ開始
    crate::kernel::process::schedule(shell_process);
    
    // この先には到達しない
    loop { halt(); }
}
```

### テスト計画

#### エンドツーエンドテスト

```rust
#[test_case]
fn test_user_shell_interaction() {
    // QEMUでシェルを起動
    // 1. "help" コマンド実行
    // 2. 出力を検証
    // 3. "echo test" コマンド実行
    // 4. 出力を検証
}
```

#### デバッグ出力による検証

```rust
// syscall_handler内にログ追加
pub extern "C" fn syscall_handler(...) -> i64 {
    debug!("Syscall {} from PID {}, CS={:#x}",
        num, current_pid(), read_cs());
    // CS=0x23 (Ring 3) であることを確認
    
    // ...
}
```

### 成功基準

- ✅ シェルがユーザー空間（Ring 3）で実行される
- ✅ すべてのI/Oがシステムコール経由
- ✅ カーネルパニックが発生しない
- ✅ コマンドが正常に実行される
- ✅ CS=0x23（Ring 3）が確認できる

---

## リスク管理

### 主要リスク

| リスク | 影響 | 緩和策 |
|--------|------|--------|
| **デバッグの複雑化** | 高 | シリアル出力による詳細ログ、GDB統合 |
| **パフォーマンス低下** | 中 | ベンチマーク測定、ホットパス最適化 |
| **メモリ使用量増加** | 中 | Copy-on-Write、カーネルメモリ共有 |
| **互換性破壊** | 低 | 段階的移行、既存コード維持 |

### 回避不可能な変更

1. **シェルの再コンパイル**: ユーザーランドライブラリ使用
2. **起動フロー変更**: プロセス生成とスケジューラ起動
3. **メモリレイアウト変更**: ユーザー/カーネル空間分離

### ロールバック戦略

各Phaseで以下を維持：
- ✅ Gitタグ作成（例: `v0.4.0-phase1`）
- ✅ 既存機能のテストパス
- ✅ ビルド成功の維持

問題発生時は前のPhaseに戻す。

---

## パフォーマンス目標

### ベンチマーク項目

| 項目 | 現在 | 目標 | 測定方法 |
|------|------|------|---------|
| システムコールレイテンシ | N/A | <500ns | rdtsc使用 |
| コンテキストスイッチ | N/A | <1μs | タイムスタンプ比較 |
| メモリオーバーヘッド | 0MB | <10MB | ページテーブル分 |

### 最適化方針

1. **Phase 1-3**: 正確性優先、パフォーマンスは後回し
2. **Phase 4**: プロファイリングとホットパス特定
3. **Post-Launch**: 段階的最適化

---

## マイルストーン

| Phase | 期間目安 | 成果物 |
|-------|---------|--------|
| **Phase 1** | 2-3日 | システムコール機構動作 |
| **Phase 2** | 3-4日 | プロセス管理とメモリ分離 |
| **Phase 3** | 1-2日 | ユーザーランドライブラリ |
| **Phase 4** | 2-3日 | シェル移行と統合テスト |
| **合計** | **8-12日** | 完全なRing分離 |

---

## 次のステップ（この設計書承認後）

1. ✅ Phase 1の詳細実装計画作成
2. ✅ `syscall.rs`のスケルトン作成
3. ✅ MSR初期化コード実装
4. ✅ システムコールエントリポイント実装（アセンブリ）
5. ✅ 最初のシステムコール（`sys_write`）実装
6. ✅ QEMUで動作テスト

---

## 参考資料

- [Intel SDM Vol.3 Chapter 5: Protection](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)
- [x86_64 System V ABI](https://gitlab.com/x86-psABIs/x86-64-ABI)
- [Linux Syscall ABI](https://man7.org/linux/man-pages/man2/syscall.2.html)
- [OSDev Wiki: System Calls](https://wiki.osdev.org/System_Calls)

---

**設計承認者**: ________________  
**承認日**: ________________
