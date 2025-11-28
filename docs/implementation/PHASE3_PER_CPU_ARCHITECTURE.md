# Phase 3: swapgs-based Per-CPU Data Architecture

## 概要

Phase 3では、syscall エントリポイントを`swapgs`ベースのPer-CPUデータ構造に移行しました。
これにより、真のSMP（対称型マルチプロセッシング）サポートの基盤が整いました。

## 変更点

### 1. 新規モジュール: `per_cpu.rs`

```
crates/kernel/src/arch/x86_64/per_cpu.rs
```

Per-CPUデータ構造を定義・管理するモジュール：

```rust
#[repr(C, align(64))]
pub struct PerCpuData {
    // オフセット 0x00: syscall時のUser RSP一時保存領域
    pub user_rsp_scratch: AtomicU64,
    
    // オフセット 0x08: このCPUのカーネルスタックトップ
    pub kernel_stack_top: AtomicU64,
    
    // オフセット 0x10: ユーザーGSベース（ネスト対応）
    pub user_gs_base: AtomicU64,
    
    // オフセット 0x18: CPU ID
    pub cpu_id: u64,
    
    // オフセット 0x20: 現在のタスクポインタ
    pub current_task: AtomicU64,
    
    // オフセット 0x28: TSS RSP0
    pub tss_rsp0: AtomicU64,
    
    // オフセット 0x30: syscallカウンタ
    pub syscall_count: AtomicU64,
    
    // ...
}
```

### 2. syscall.rs の変更

#### Before (Phase 2)
```asm
mov r15, rsp                              ; User RSPを一時保存
mov rsp, qword ptr [rip + CURRENT_KERNEL_STACK]  ; グローバル変数からロード
; ...
```

#### After (Phase 3)
```asm
swapgs                                    ; User GS <-> Kernel GS
mov qword ptr gs:[0x00], rsp              ; User RSPをPer-CPU領域に保存
mov rsp, qword ptr gs:[0x08]              ; Per-CPUからカーネルスタックをロード
; ...
swapgs                                    ; GS復元
sysretq
```

### 3. MSR設定

`IA32_KERNEL_GS_BASE` (MSR 0xC0000102) にPer-CPUデータのアドレスを設定。
`swapgs`命令実行時に、このMSRと`IA32_GS_BASE`の値が交換される。

## メモリレイアウト

```
Per-CPU Data Structure (64バイトアライン)
┌─────────────────────────────────────────┐
│ 0x00: user_rsp_scratch    (8 bytes)     │  ← syscall時のUser RSP一時保存
│ 0x08: kernel_stack_top    (8 bytes)     │  ← カーネルスタックポインタ
│ 0x10: user_gs_base        (8 bytes)     │  ← ネスト対応用
│ 0x18: cpu_id              (8 bytes)     │  ← CPU識別子
│ 0x20: current_task        (8 bytes)     │  ← 現在のプロセスポインタ
│ 0x28: tss_rsp0            (8 bytes)     │  ← TSS用
│ 0x30: syscall_count       (8 bytes)     │  ← 統計用
│ 0x38: last_syscall_time   (8 bytes)     │  ← パフォーマンス監視
│ 0x40: _padding            (32 bytes)    │  ← キャッシュライン境界調整
└─────────────────────────────────────────┘
Total: 128 bytes (2 cache lines)
```

## コンパイル時検証

オフセット値の正確性はコンパイル時に検証されます：

```rust
const _: () = {
    use core::mem::offset_of;
    
    assert!(offset_of!(PerCpuData, user_rsp_scratch) == offset::USER_RSP_SCRATCH);
    assert!(offset_of!(PerCpuData, kernel_stack_top) == offset::KERNEL_STACK_TOP);
    // ...
};
```

## SMP拡張計画

現在は単一CPU向けですが、SMP拡張は以下の手順で行えます：

1. **AP (Application Processor) 起動時**
   - 各CPUに一意のID割り当て
   - Per-CPUデータ配列の該当エントリを初期化
   - `IA32_KERNEL_GS_BASE`を設定

2. **CPU ID取得**
   ```asm
   ; 現在のCPU IDを取得
   mov rax, qword ptr gs:[0x18]
   ```

3. **ロックレス操作**
   - Per-CPUデータは各CPUが専有するため、ロック不要
   - `AtomicU64`は他CPUからの可視性保証用

## パフォーマンス影響

| 操作 | Phase 2 | Phase 3 | 改善 |
|------|---------|---------|------|
| User RSP保存 | mov r15, rsp (1 cycle) | mov gs:[0], rsp (1 cycle) | - |
| カーネルスタックロード | [rip+offset] (2-3 cycles) | gs:[8] (1 cycle) | ~50% |
| GS切り替え | N/A | swapgs (20-30 cycles) | 初期コスト |

短いsyscallでは`swapgs`のオーバーヘッドが目立ちますが、
SMP環境では同期オーバーヘッドが消滅するため、
全体としてスケーラビリティが大幅に向上します。

## 実装状況

**✅ Phase 3.1 完了: 2025年実装**

### 検証結果

QEMU実行テストで以下を確認:
- Per-CPU初期化成功 (`IA32_KERNEL_GS_BASE: 0x27e080`)
- swapgsベースのsyscall entry/exit動作
- io_uringテスト全パス
- プロセス正常終了 (exit code=0)

```
[Per-CPU] Initialized for CPU 0
  Per-CPU data at: 0x27e080
  Kernel stack top: 0x2820f0
  IA32_KERNEL_GS_BASE: 0x27e080
[OK] Syscall mechanism initialized (swapgs-based)
...
[SYSCALL-ENTRY] num=0, args=(0x1, 0x403050, 0x10, 0x0, 0x0, 0x0)
[SYSCALL] Dispatching syscall 0 with args=(1, 4206672, 16, 0, 0, 0)
...
[SYSCALL] sys_exit: code=0
[Process] Terminated PID=1 with code=0
```

### 重要な修正点

`validate_syscall_context`でのGS base検証:
- swapgs実行後は `IA32_GS_BASE` (not `IA32_KERNEL_GS_BASE`)を確認
- ユーザーGSは通常0、カーネルGSがPer-CPUアドレス

## 関連ファイル

- `crates/kernel/src/arch/x86_64/per_cpu.rs` - Per-CPUデータ定義
- `crates/kernel/src/arch/x86_64/syscall.rs` - swapgsベースのエントリポイント
- `crates/kernel/src/arch/x86_64/mod.rs` - モジュールエクスポート
- `crates/kernel/src/arch/x86_64/tss.rs` - TSS連携

## 次のステップ

1. **SQPOLL実装** - io_uringのポーリングモード
2. **Registered Buffers** - 事前登録バッファによるcopy_from_user最適化
3. **SMP起動** - マルチコアサポート
