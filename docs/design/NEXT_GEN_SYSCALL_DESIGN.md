# 次世代システムコール設計：Ring-Based Async Protocol

**作成日**: 2025年11月29日  
**改訂日**: 2025年11月29日（完全Rustネイティブ化版）  
**対象OS**: Tiny OS (x86_64 UEFI)  
**目的**: **C互換性ゼロ**、Rust の特性を最大限活かした「完全に効率的なシステムコール」の実現

---

## 1. エグゼクティブサマリー

### 設計哲学

> **「互換性の足かせを外し、Rustの型システムと所有権モデルを最大限に活かす」**

TinyOS は新規OSであり、POSIX/C互換性を維持する理由がありません。
この設計では**整数FD、errno、C ABI を完全に廃止**し、Rustネイティブな設計に移行します。

### 現状分析結果

TinyOS は既に **非常に優れた io_uring ベースの非同期システムコール基盤** を持っています：

| コンポーネント | 状態 | 評価 |
|---------------|------|------|
| **SQ/CQ リングバッファ** | ✅ 実装済み | 共有メモリ、Lock-free、TOCTOU保護 |
| **登録済みバッファ** | ✅ 実装済み | ゼロコピー対応、参照カウント付き |
| **SQPOLL** | ✅ 実装済み | カーネルポーリング、Syscallレス操作 |
| **async/await 統合** | ✅ 実装済み | `IoUringFuture` による統合 |
| **型安全な ABI** | ✅ 実装済み | `#[repr(C)]` + `OpCode` enum |

### 完全Rustネイティブ化への変更点

| 項目 | 従来の計画 | 問題点 | **新計画** |
|------|-----------|--------|-----------|
| **FD互換レイヤー** | `fd_to_capability()` で変換 | 整数FDの概念が残る | **即座に廃止**、全てCapability化 |
| **ABI境界** | `#[repr(C)]` 構造体 | C互換を前提 | **Rust専用ABI**に移行 |
| **エラー型** | `i32` errno | POSIXの遺産 | **最初から `Result<T, E>`** |
| **syscall番号** | 整数定数 | 型安全性なし | **型付きトレイト** |

### ギャップ分析（急進的アプローチ）

| 提案の機能 | 現状 | アクション | 優先度 |
|-----------|------|-----------|--------|
| **整数FD廃止** | ❌ FD は単なる整数 | **完全削除** | 🔴 Phase 0 |
| **Rust専用ABI** | ❌ C互換前提 | **新規設計** | 🔴 Phase 0 |
| **型付きsyscall番号** | ❌ 整数定数 | **トレイト化** | 🔴 Phase 0 |
| **Capability-based Security** | ❌ 未実装 | **完全移行** | 🔴 Phase 1 |
| **型付きハンドル（Move Semantics）** | ❌ 未適用 | **所有権モデル適用** | 🔴 Phase 1 |
| **Doorbell Mechanism** | 🟡 syscall ベース | **共有メモリ化** | 🟡 Phase 2 |
| **Result<T, E> 型エラー** | 🟡 ABI レベルでは i32 | **AbiResult<T,E>** | 🟡 Phase 3 |

---

## 2. Phase 0: カーネルコア完全Rust化（Phase 1の前に実施）

**期間**: 1週間  
**目的**: C互換性の残滓を完全に排除し、Rust専用基盤を構築

### 2.1 整数FDの完全廃止

```rust
// ❌ 削除: 互換性レイヤーは作らない
// pub fn fd_to_capability(fd: i32) -> Option<Handle<FileResource>>

// ✅ 最初から Capability のみ
impl Process {
    pub fn capability_table(&self) -> &CapabilityTable {
        &self.capabilities
    }
    
    // FD という概念自体を持たない
    // pub fn file_descriptor_table(&self) -> ... // ❌ 削除
}
```

**削除対象ファイル/コード**:

- `kernel/src/kernel/process/mod.rs` の `file_descriptors: BTreeMap<u64, ...>`
- `kernel/src/kernel/fs/mod.rs` の `FileDescriptor` トレイト（`Capability` に置換）
- `kernel/src/kernel/io_uring/handlers.rs` の FD ベースの処理

### 2.2 Rust専用ABIの定義

POSIXやC互換を考えず、**Rustの型システムを最大限活用**：

```rust
// crates/kernel/src/abi/native.rs (新規)

/// Rust専用システムコール ABI
/// 
/// C互換性を捨て、Rustの型システムを最大限活用
pub mod native {
    use core::marker::PhantomData;
    
    /// システムコール引数（型パラメータで種類を表現）
    #[repr(transparent)]
    pub struct SyscallArg<T> {
        value: u64,
        _phantom: PhantomData<T>,
    }
    
    impl<T> SyscallArg<T> {
        pub const fn new(value: u64) -> Self {
            Self { value, _phantom: PhantomData }
        }
        
        pub const fn raw(&self) -> u64 {
            self.value
        }
    }
    
    /// Capability引数（型安全）
    pub type CapArg<R> = SyscallArg<Handle<R>>;
    
    /// ポインタ引数（型安全）
    pub type PtrArg<T> = SyscallArg<*const T>;
    
    /// 長さ引数
    pub type LenArg = SyscallArg<usize>;
    
    /// システムコールディスパッチ（トレイトベース）
    pub trait SyscallDispatch {
        type Output;
        
        /// システムコールを実行
        /// 
        /// 型パラメータで引数の型を強制
        fn dispatch(args: &SyscallArgs) -> Result<Self::Output, SyscallError>;
    }
    
    /// Read システムコール
    pub struct ReadSyscall;
    
    impl SyscallDispatch for ReadSyscall {
        type Output = usize; // 読み取ったバイト数
        
        fn dispatch(args: &SyscallArgs) -> Result<Self::Output, SyscallError> {
            // 型安全な引数抽出
            let cap: CapArg<FileResource> = args.get(0)?;
            let buf_idx: SyscallArg<u32> = args.get(1)?;
            let len: LenArg = args.get(2)?;
            
            // Capability検証 → リソースアクセス → 結果返却
            todo!()
        }
    }
}
```

### 2.3 型付きシステムコール番号

整数定数ではなく、**型で表現**：

```rust
// crates/kernel/src/abi/syscall_numbers.rs (新規)

/// システムコール番号（型で表現）
pub trait SyscallNumber {
    const NUMBER: u64;
    const NAME: &'static str;
}

/// Read システムコール
pub struct SYS_READ;
impl SyscallNumber for SYS_READ {
    const NUMBER: u64 = 0;
    const NAME: &'static str = "read";
}

/// Write システムコール
pub struct SYS_WRITE;
impl SyscallNumber for SYS_WRITE {
    const NUMBER: u64 = 1;
    const NAME: &'static str = "write";
}

/// io_uring セットアップ
pub struct SYS_IO_URING_SETUP;
impl SyscallNumber for SYS_IO_URING_SETUP {
    const NUMBER: u64 = 12;
    const NAME: &'static str = "io_uring_setup";
}

// コンパイル時に番号の重複を検出可能
const _: () = {
    assert!(SYS_READ::NUMBER != SYS_WRITE::NUMBER);
    assert!(SYS_READ::NUMBER != SYS_IO_URING_SETUP::NUMBER);
};
```

---

## 3. 現在のアーキテクチャ（移行前）

```text
┌─────────────────────────────────────────────────────────────────────┐
│                         User Space (Ring 3)                         │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │ libuser/src/async_io.rs, io_uring.rs, ring_io.rs             │   │
│  │   - AsyncContext, IoUring, Ring                               │   │
│  │   - submit(), flush(), get_completion()                       │   │
│  └──────────────────────────────────────────────────────────────┘   │
│           │ syscall 12/13/14 (io_uring_setup/enter/register)        │
└───────────┼─────────────────────────────────────────────────────────┘
            ▼
┌─────────────────────────────────────────────────────────────────────┐
│                        Kernel Space (Ring 0)                        │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │ kernel/src/kernel/io_uring/                                    │ │
│  │   - ring.rs (IoUring: SQ/CQ管理)                              │ │
│  │   - context.rs (IoUringContext: プロセス単位の管理)            │ │
│  │   - handlers.rs (dispatch_sqe: NOP/Read/Write/Mmap等)         │ │
│  │   - registered_buffers.rs (ゼロコピーバッファ)                │ │
│  │   - sqpoll.rs (SQPOLL Worker)                                 │ │
│  └────────────────────────────────────────────────────────────────┘ │
│  ┌────────────────────────────────────────────────────────────────┐ │
│  │ kernel/src/kernel/async/io_uring_future.rs                    │ │
│  │   - IoUringFuture (async/await 統合)                          │ │
│  │   - complete_operation() (Waker 発火)                         │ │
│  └────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

### 現在のデータフロー

```text
1. ユーザーが AsyncContext::submit(op) を呼ぶ
2. SQE を共有メモリの SQ に書き込み
3. io_uring_enter() syscall で通知（または SQPOLL がポーリング）
4. カーネルが SQE を処理 → CQE を CQ に書き込み
5. ユーザーが get_completion() で結果を取得
```

---

## 4. Phase 1: Capability 完全移行（1-2週間）

**目的**: FD の単なる整数から「型付き権限付きトークン」への完全移行。**互換レイヤーなし。**

### 4.1 型付きハンドル設計

```rust
// crates/kernel/src/kernel/capability/mod.rs (新規)

use core::marker::PhantomData;

/// Capability の権限フラグ
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rights(pub u64);

impl Rights {
    pub const NONE: Self = Self(0);
    pub const READ: Self = Self(1 << 0);
    pub const WRITE: Self = Self(1 << 1);
    pub const SEEK: Self = Self(1 << 2);
    pub const MAP: Self = Self(1 << 3);
    pub const DUP: Self = Self(1 << 4);
    pub const TRANSFER: Self = Self(1 << 5);
    pub const CLOSE: Self = Self(1 << 6);
    
    // ネットワーク権限
    pub const NET_CONNECT: Self = Self(1 << 16);
    pub const NET_ACCEPT: Self = Self(1 << 17);
    
    // プリセット
    pub const READ_ONLY: Self = Self(Self::READ.0 | Self::SEEK.0);
    pub const READ_WRITE: Self = Self(Self::READ.0 | Self::WRITE.0 | Self::SEEK.0);
    
    pub const fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

/// リソースの種類を示すマーカー型
pub trait ResourceKind: Send + Sync + 'static {
    const TYPE_ID: u32;
}

pub struct FileResource;
impl ResourceKind for FileResource { const TYPE_ID: u32 = 1; }

pub struct SocketResource;
impl ResourceKind for SocketResource { const TYPE_ID: u32 = 2; }

pub struct BufferResource;
impl ResourceKind for BufferResource { const TYPE_ID: u32 = 4; }

/// 型安全な Capability ハンドル（ゼロコスト抽象化）
#[repr(transparent)]
pub struct Handle<R: ResourceKind> {
    id: u64,
    _phantom: PhantomData<R>,
}

impl<R: ResourceKind> Handle<R> {
    pub(crate) fn new(id: u64) -> Self {
        Self { id, _phantom: PhantomData }
    }
    
    pub fn raw(&self) -> u64 { self.id }
    
    pub unsafe fn from_raw(id: u64) -> Self { Self::new(id) }
}

// Clone/Copy 非実装 → 所有権の移動を強制
impl<R: ResourceKind> Drop for Handle<R> {
    fn drop(&mut self) {
        // 自動クローズ
    }
}
```

### 4.2 SubmissionEntryV2（Capability ベース）

```rust
// crates/kernel/src/abi/io_uring_v2.rs (新規)

/// 次世代 SQE：Capability ベース
#[repr(C, align(64))]
pub struct SubmissionEntryV2 {
    pub opcode: u8,
    pub flags: u8,
    pub ioprio: u16,
    
    /// Capability ハンドルID（FD を完全に置換）
    pub capability_id: u64,
    
    pub off: u64,
    pub buf_index: u32,
    pub len: u32,
    pub op_flags: u32,
    pub user_data: u64,
    pub _reserved: [u64; 2],
}

const _: () = assert!(core::mem::size_of::<SubmissionEntryV2>() == 64);
```

---

## 4. Phase 2: Doorbell / ゼロシステムコールモード（1週間）

**目的**: SQPOLL の完全活用と Doorbell 方式の実装

### 5.1 Doorbell メモリ領域

```rust
// crates/kernel/src/kernel/io_uring/doorbell.rs (新規)

use core::sync::atomic::{AtomicU32, AtomicBool, Ordering};

/// Doorbell 構造体（共有メモリにマップ）
#[repr(C, align(4096))]
pub struct Doorbell {
    /// ユーザーが書き込む doorbell 値
    /// 非ゼロを書き込むとカーネルに通知
    pub ring: AtomicU32,
    
    /// カーネルがセットする「ウェイクアップ必要」フラグ
    /// SQPOLL がアイドル時にセットされる
    pub needs_wakeup: AtomicBool,
    
    /// カーネルがセットする「CQ にエントリあり」フラグ
    pub cq_ready: AtomicBool,
    
    /// パディング
    _pad: [u8; 4096 - 6],
}

impl Doorbell {
    pub const fn new() -> Self {
        Self {
            ring: AtomicU32::new(0),
            needs_wakeup: AtomicBool::new(false),
            cq_ready: AtomicBool::new(false),
            _pad: [0; 4096 - 6],
        }
    }
    
    /// ユーザーが doorbell をリング（syscall 不要）
    pub fn ring_doorbell(&self) {
        self.ring.fetch_add(1, Ordering::Release);
    }
    
    /// カーネルが doorbell をチェック
    pub fn check_and_clear(&self) -> bool {
        self.ring.swap(0, Ordering::AcqRel) > 0
    }
}
```

### 5.2 SQPOLL の強化（完全非同期化）

```rust
// crates/kernel/src/kernel/io_uring/sqpoll_v2.rs (新規)

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

/// SQPOLL ワーカー（完全非同期化）
pub struct SqpollWorkerV2 {
    ring: Arc<IoUring>,
    doorbell: Arc<Doorbell>,
    executor: Arc<KernelExecutor>,
}

impl SqpollWorkerV2 {
    /// メインループ（Future として実装）
    pub async fn run(&self) {
        loop {
            // Doorbell待機（非同期）
            self.doorbell.wait_for_ring().await;
            
            // SQE処理（非同期）
            while let Some(sqe) = self.ring.dequeue_sqe().await {
                // 各オペレーションを非同期タスクとしてスポーン
                let task = self.process_sqe(sqe);
                self.executor.spawn(task);
            }
        }
    }
    
    /// SQE処理（Future を返す）
    async fn process_sqe(&self, sqe: SubmissionEntryV2) -> CompletionEntryV2 {
        match OpCode::from_u8(sqe.opcode) {
            Some(OpCode::Read) => self.handle_read(sqe).await,
            Some(OpCode::Write) => self.handle_write(sqe).await,
            Some(OpCode::Nop) => self.handle_nop(sqe).await,
            // ...
            _ => CompletionEntryV2::error(sqe.user_data, SyscallError::NotImplemented),
        }
    }
    
    /// Read処理（完全非同期）
    async fn handle_read(&self, sqe: SubmissionEntryV2) -> CompletionEntryV2 {
        // Capability検証
        let cap = unsafe { 
            Handle::<FileResource>::from_raw(sqe.capability_id) 
        };
        
        let entry = match self.verify_capability(&cap, Rights::READ) {
            Ok(e) => e,
            Err(e) => return CompletionEntryV2::error(sqe.user_data, e),
        };
        
        let file = match entry.resource.downcast_ref::<VfsFile>() {
            Some(f) => f,
            None => return CompletionEntryV2::error(sqe.user_data, SyscallError::WrongCapabilityType),
        };
        
        // 非同期Read（ブロックしない）
        match file.read_async(sqe.off, sqe.len).await {
            Ok(n) => CompletionEntryV2::success(sqe.user_data, n as i32),
            Err(e) => CompletionEntryV2::error(sqe.user_data, e),
        }
    }
}
```

### 5.3 完全ゼロシステムコール I/O フロー

```text
┌─────────────────────────────────────────────────────────────────────┐
│                         User Space                                   │
│                                                                      │
│  1. Write SQE to SQ (shared memory)                                 │
│  2. Update SQ tail (atomic)                                         │
│  3. Write to doorbell (shared memory) ← NO SYSCALL                  │
│  4. Poll CQ tail (atomic)                                           │
│  5. Read CQE from CQ (shared memory)                                │
│  6. Update CQ head (atomic)                                         │
│                                                                      │
└───────────────────────────────────────────────────────────────────-─┘
                              │ doorbell write detected
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│                        Kernel Space (SQPOLL)                         │
│                                                                      │
│  SQPOLL Worker:                                                      │
│  1. Poll doorbell / SQ tail                                         │
│  2. Copy SQE to kernel (TOCTOU protection)                          │
│  3. Process operation                                                │
│  4. Write CQE to CQ                                                 │
│  5. Set cq_ready flag                                               │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### 5.4 ユーザー空間 API（syscall なし）

```rust
// crates/libuser/src/zero_syscall_io.rs (新規)

/// ゼロシステムコール I/O インターフェース
pub struct ZeroSyscallIo {
    sq: &'static mut SubmissionQueue,
    cq: &'static CompletionQueue,
    doorbell: &'static Doorbell,
    buffers: RegisteredBuffers,
}

impl ZeroSyscallIo {
    /// Read（syscall なし）
    pub async fn read(
        &mut self,
        cap: Handle<FileResource>,
        buf_idx: u32,
        len: usize,
    ) -> Result<usize, SyscallError> {
        // SQE作成
        let sqe = SubmissionEntryV2 {
            opcode: OpCode::Read as u8,
            capability_id: cap.raw(),
            buf_index: buf_idx,
            len: len as u32,
            user_data: self.allocate_user_data(),
            ..Default::default()
        };
        
        // SQ に書き込み（共有メモリ）
        unsafe { self.sq.write(sqe); }
        
        // Doorbell を鳴らす（syscall なし！）
        self.doorbell.ring_doorbell();
        
        // CQ をポーリング（syscall なし！）
        loop {
            if let Some(cqe) = unsafe { self.cq.try_read(sqe.user_data) } {
                return cqe.result.into();
            }
            
            // Yield（他のタスクに譲る）
            core::future::yield_now().await;
        }
    }
}
```

---

## 5. Phase 3: Result<T, E> 型の徹底（1週間）

**目的**: errno 整数を完全に廃止し、型安全な Result に移行

### 6.1 ABI層での Result 表現

```rust
// crates/kernel/src/abi/result.rs (新規)

/// ABI越しに Result を安全に渡す
#[repr(C)]
pub struct AbiResult<T, E> {
    tag: u8, // 0 = Ok, 1 = Err
    _pad: [u8; 7],
    data: AbiResultData<T, E>,
}

#[repr(C)]
union AbiResultData<T, E> {
    ok: core::mem::ManuallyDrop<T>,
    err: core::mem::ManuallyDrop<E>,
}

impl<T, E> From<Result<T, E>> for AbiResult<T, E> {
    fn from(result: Result<T, E>) -> Self {
        match result {
            Ok(val) => Self {
                tag: 0,
                _pad: [0; 7],
                data: AbiResultData {
                    ok: core::mem::ManuallyDrop::new(val),
                },
            },
            Err(err) => Self {
                tag: 1,
                _pad: [0; 7],
                data: AbiResultData {
                    err: core::mem::ManuallyDrop::new(err),
                },
            },
        }
    }
}

impl<T, E> From<AbiResult<T, E>> for Result<T, E> {
    fn from(abi: AbiResult<T, E>) -> Self {
        match abi.tag {
            0 => Ok(unsafe { core::mem::ManuallyDrop::into_inner(abi.data.ok) }),
            1 => Err(unsafe { core::mem::ManuallyDrop::into_inner(abi.data.err) }),
            _ => panic!("Invalid AbiResult tag"),
        }
    }
}
```

### 6.2 CQE での Result 表現

```rust
// crates/kernel/src/abi/io_uring_v2.rs (拡張)

/// 完了エントリV2（Result型を直接表現）
#[repr(C)]
pub struct CompletionEntryV2 {
    pub user_data: u64,
    
    /// 結果（Ok時は成功値、Err時はエラー）
    pub result: AbiResult<i32, SyscallError>,
    
    pub flags: u32,
    _pad: u32,
}

impl CompletionEntryV2 {
    pub fn success(user_data: u64, value: i32) -> Self {
        Self {
            user_data,
            result: Ok(value).into(),
            flags: 0,
            _pad: 0,
        }
    }
    
    pub fn error(user_data: u64, err: SyscallError) -> Self {
        Self {
            user_data,
            result: Err(err).into(),
            flags: 0,
            _pad: 0,
        }
    }
}

// ユーザー側での使用例
let cqe = ring.wait_completion().await;
let result: Result<i32, SyscallError> = cqe.result.into();
match result {
    Ok(bytes) => println!("Read {} bytes", bytes),
    Err(SyscallError::InvalidCapability) => eprintln!("Invalid capability"),
    Err(e) => eprintln!("Error: {:?}", e),
}
```

### 6.3 型付きエラー（完全版）

```rust
// crates/kernel/src/abi/error.rs (新規)

/// システムコールエラー（完全Rustネイティブ）
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallError {
    // 一般エラー
    InvalidArgument = 1,
    OutOfMemory = 2,
    PermissionDenied = 3,
    NotFound = 4,
    Busy = 5,
    Interrupted = 6,
    
    // I/O エラー
    IoError = 10,
    WouldBlock = 11,
    BrokenPipe = 12,
    ConnectionReset = 13,
    
    // Capability エラー
    InvalidCapability = 20,
    InsufficientRights = 21,
    WrongCapabilityType = 22,
    CapabilityRevoked = 23,
    
    // io_uring エラー
    QueueFull = 30,
    BufferNotRegistered = 31,
    InvalidBufferIndex = 32,
    
    // システムエラー
    NotImplemented = 255,
}

// errno は存在しない！
// impl SyscallError { fn to_errno() } // ❌ 削除
```

---

## 6. 完全Rustネイティブ化のメリット

| 側面 | メリット | 具体例 |
|------|---------|--------|
| **型安全性** | コンパイル時エラー検出 | 間違った型のCapabilityは渡せない |
| **所有権** | Use-after-freeの根絶 | Handleのドロップで自動クローズ |
| **ゼロコスト** | 実行時オーバーヘッドなし | `Handle<T>` は単なる `u64` |
| **非同期** | ネイティブ `Future` 統合 | `async fn` がそのまま使える |
| **保守性** | Rustエコシステム活用 | `cargo`, `rustdoc`, `clippy` |
| **エラー処理** | パターンマッチ強制 | `Result` の `?` 演算子 |

---

## 7. 改訂ロードマップ（8週間計画）

### Week 1: Phase 0（完全Rust化準備）

- [ ] 整数FD完全廃止の設計
- [ ] Rust専用ABI定義（`native.rs`）
- [ ] 型付きシステムコール番号（`syscall_numbers.rs`）

### Week 2-3: Phase 1（Capability移行）

- [ ] `capability/mod.rs` 実装（Rights, Handle<R>）
- [ ] `capability/table.rs` 実装（CapabilityTable）
- [ ] 既存コード一括書き換え（互換レイヤーなし！）

### Week 4: Phase 1続き + テスト

- [ ] `SubmissionEntryV2`, `CompletionEntryV2` 実装
- [ ] io_uring ハンドラの Capability 対応
- [ ] 単体テスト、統合テスト

### Week 5-6: Phase 2（Doorbell）

- [ ] `doorbell.rs` 実装
- [ ] SQPOLL v2 強化
- [ ] ゼロシステムコールモードのテスト

### Week 7: Phase 3（Result型）

- [ ] `AbiResult<T, E>` 実装
- [ ] CQE v2 移行
- [ ] errno の完全廃止

### Week 8: 統合・最適化

- [ ] エンドツーエンドテスト
- [ ] パフォーマンスベンチマーク
- [ ] ドキュメント完成

---

## 8. 最終的なアーキテクチャ

```text
┌─────────────────────────────────────────────────────────────┐
│                    User Space (Rust)                        │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ async fn main() {                                    │   │
│  │   let file: Handle<FileResource> =                   │   │
│  │     open("/data", Rights::READ_ONLY).await?;         │   │
│  │                                                       │   │
│  │   let data = io.read(file, buf_idx, 1024).await?;    │   │
│  │   // ↑ syscall なし！Doorbell + 共有メモリのみ       │   │
│  │ }                                                     │   │
│  └──────────────────────────────────────────────────────┘   │
│           │ Doorbell Write (共有メモリ)                     │
└───────────┼─────────────────────────────────────────────────┘
            ▼
┌─────────────────────────────────────────────────────────────┐
│                   Kernel Space (Rust)                       │
│  ┌──────────────────────────────────────────────────────┐   │
│  │ async fn sqpoll_worker() {                           │   │
│  │   loop {                                             │   │
│  │     doorbell.wait_for_ring().await;                  │   │
│  │     let sqe = sq.dequeue().await;                    │   │
│  │     executor.spawn(handle_sqe(sqe));                 │   │
│  │   }                                                   │   │
│  │ }                                                     │   │
│  │                                                       │   │
│  │ async fn handle_read(sqe) -> Result<usize, Error> {  │   │
│  │   let cap = verify_capability(sqe.cap_id)?;          │   │
│  │   file.read_async(offset, len).await                 │   │
│  │ }                                                     │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

**完全にRustネイティブ。C互換性ゼロ。最大効率。**

---

## 9. 設計比較表（最終版）

| 特徴 | POSIX/Linux | 現在の TinyOS | **次世代 TinyOS** |
|-----|-------------|--------------|-------------------|
| **呼び出し方法** | 同期 `syscall` | 非同期リング | **Doorbell + 共有メモリ** |
| **データ転送** | ポインタ+コピー | 登録済みバッファ | **登録済みバッファ (DMA可)** |
| **多重化** | `epoll`/`select` | `IoUringFuture` | **ネイティブ `async/await`** |
| **リソース識別** | 整数 (FD) | 整数 (FD) | **型付き `Handle<R>`** |
| **権限管理** | UNIX パーミッション | なし | **Capability + Rights** |
| **エラー処理** | `-1` + `errno` | `-errno` | **`Result<T, SyscallError>`** |
| **所有権** | なし | なし | **Move Semantics（自動クローズ）** |
| **型安全性** | なし | 部分的 | **コンパイル時検証** |
| **C互換性** | 前提 | 維持 | **なし（Rust専用）** |

---

## 10. 将来の拡張（長期計画）

### 11.1 In-Kernel Scripting (eBPF スタイル)

```rust
// 将来の構想

/// カーネル注入可能なプログラム
pub trait KernelProgram: Send + Sync {
    /// プログラムを検証（安全性チェック）
    fn verify(&self) -> Result<(), VerificationError>;
    
    /// プログラムを実行
    fn execute(&self, ctx: &mut ExecutionContext) -> ProgramResult;
}
```

### 11.2 DMA 直結（ハードウェア依存）

```text
NVMe SSD との DMA フロー:

1. ユーザーが登録済みバッファで Read 要求
2. カーネルが物理アドレスを NVMe コマンドに設定
3. NVMe コントローラが直接バッファに DMA
4. CPU コピーなしで完了
```

---

## 11. 次のアクション（即座に開始）

1. **`crates/kernel/src/abi/native.rs` 作成** - Rust専用ABI
2. **`crates/kernel/src/abi/syscall_numbers.rs` 作成** - 型付きsyscall番号
3. **`crates/kernel/src/kernel/capability/` ディレクトリ作成**
4. **既存 FD 関連コードの削除リスト作成**

---

## 12. 参考資料

- [io_uring の設計](https://kernel.dk/io_uring.pdf)
- [Capability-based Security](https://en.wikipedia.org/wiki/Capability-based_security)
- [FreeBSD Capsicum](https://www.freebsd.org/cgi/man.cgi?capsicum)
- [seL4 Capability Model](https://docs.sel4.systems/Tutorials/capabilities.html)
- [Rust 所有権システム](https://doc.rust-lang.org/book/ch04-00-understanding-ownership.html)
- [Redox OS Capability Design](https://doc.redox-os.org/book/ch04-08-capability.html)

---

**設計承認者**: ________________  
**承認日**: ________________
