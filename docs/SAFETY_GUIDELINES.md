# セーフティガイドライン

## 目的

このドキュメントは、Tiny OSカーネル開発における安全性のベストプラクティスを定義します。メモリ安全性、スレッド安全性、`unsafe`コードの適切な使用方法について説明します。

## `unsafe`コードの原則

### 基本ルール

1. **最小限の使用** - `unsafe`は絶対に必要な場合のみ使用
2. **明示的なunsafeブロック** - `#![deny(unsafe_op_in_unsafe_fn)]`を有効化
3. **詳細なドキュメント** - すべての`unsafe`に対して`# Safety`セクションを記述
4. **不変条件の明示** - 何が保証されているかを明確に

### unsafe関数の書き方

```rust
/// 指定された領域を空きリストに追加
/// 
/// # Safety
/// 
/// - addr は有効なメモリアドレスである必要があります
/// - size は少なくとも ListNode を格納できるサイズである必要があります
/// - addr は ListNode のアラインメントに合っている必要があります
unsafe fn add_free_region(&mut self, addr: usize, size: usize) {
    // アラインメントとサイズの検証
    assert_eq!(
        align_up(addr, mem::align_of::<ListNode>()),
        Some(addr),
        "Heap start address must be aligned"
    );
    assert!(
        size >= mem::size_of::<ListNode>(),
        "Heap size too small"
    );
    
    unsafe {
        // 実際のunsafe操作
        let node_ptr = addr as *mut ListNode;
        node_ptr.write(node);
        self.head.next = Some(&mut *node_ptr);
    }
}
```

### unsafeブロック内での検証

**❌ 悪い例:**

```rust
unsafe {
    *ptr = value;  // ポインタの有効性を検証していない
}
```

**✅ 良い例:**

```rust
if ptr.is_null() {
    return Err(MemoryError::InvalidAddress.into());
}

unsafe {
    // Safety: ptr が null でないことを上で確認済み
    *ptr = value;
}
```

## メモリ安全性

### ポインタ操作

#### 生ポインタの検証

```rust
// ✅ 適切な検証
pub fn from_raw_parts(ptr: *mut u8, len: usize) -> KernelResult<&'static mut [u8]> {
    // null チェック
    if ptr.is_null() {
        return Err(MemoryError::InvalidAddress.into());
    }
    
    // アラインメントチェック
    if (ptr as usize) % align_of::<u8>() != 0 {
        return Err(MemoryError::MisalignedAccess.into());
    }
    
    // オーバーフローチェック
    let end = (ptr as usize).checked_add(len)
        .ok_or(MemoryError::InvalidAddress)?;
    
    unsafe {
        // Safety: 上記のすべてのチェックをパス
        Ok(core::slice::from_raw_parts_mut(ptr, len))
    }
}
```

#### アライメント要件

```rust
/// Option<usize> を返すalign_up
fn align_up(addr: usize, align: usize) -> Option<usize> {
    // alignが2の累乗でない場合はNone
    if align == 0 || (align & (align - 1)) != 0 {
        return None;
    }
    
    let mask = align.wrapping_sub(1);
    addr.checked_add(mask).map(|n| n & !mask)
}
```

### メモリリーク防止

#### RAIIパターンの使用

```rust
pub struct LockedResource<T> {
    inner: Mutex<T>,
}

impl<T> LockedResource<T> {
    pub fn lock(&self) -> MutexGuard<'_, T> {
        // MutexGuard がドロップされると自動的にロック解放
        self.inner.lock()
    }
}
```

#### 早期リターン時の注意

```rust
pub fn allocate_and_init() -> KernelResult<*mut Device> {
    let ptr = allocate()?;
    
    // 初期化に失敗した場合、割り当てたメモリを解放
    if let Err(e) = initialize(ptr) {
        unsafe {
            deallocate(ptr);
        }
        return Err(e);
    }
    
    Ok(ptr)
}
```

## スレッド安全性と並行性

### 非同期コンテキストでの安全性

#### Mutex の使用

```rust
use spin::Mutex;

// ✅ グローバル状態の保護
static GLOBAL_STATE: Mutex<State> = Mutex::new(State::new());

pub fn modify_state() {
    let mut state = GLOBAL_STATE.lock();
    state.update();
    // MutexGuard のドロップでロック自動解放
}
```

#### デッドロック回避

```rust
// ❌ デッドロックの可能性
fn bad_example() {
    let lock1 = RESOURCE_A.lock();
    let lock2 = RESOURCE_B.lock();  // 別の関数が逆順でロックするとデッドロック
}

// ✅ ロック順序の統一
fn good_example() {
    // 常にID順でロックを取得
    let (lock_first, lock_second) = if RESOURCE_A_ID < RESOURCE_B_ID {
        (RESOURCE_A.lock(), RESOURCE_B.lock())
    } else {
        (RESOURCE_B.lock(), RESOURCE_A.lock())
    };
}
```

#### Atomic操作

```rust
use core::sync::atomic::{AtomicU64, Ordering};

static TICKS: AtomicU64 = AtomicU64::new(0);

// ✅ スレッドセーフなカウンタ
pub fn increment_ticks() {
    TICKS.fetch_add(1, Ordering::Relaxed);
}
```

### 非同期タスクの安全性

```rust
// ✅ タスク内エラーの隔離
impl Executor {
    pub fn run(&self) {
        loop {
            let task_id = match self.task_queue.lock().pop_front() {
                Some(id) => id,
                None => break,
            };
            
            // タスクのパニックがExecutorを停止させないようにする
            let result = std::panic::catch_unwind(|| {
                // タスクを実行
            });
            
            if result.is_err() {
                // パニックをログ記録して継続
                log_panic(task_id);
            }
        }
    }
}
```

## ハードウェアとの相互作用

### I/Oポート操作

```rust
unsafe fn write_port(port: u16, value: u8) -> KernelResult<()> {
    // ポート番号の妥当性チェック
    if port > 0xFFFF {
        return Err(ErrorKind::InvalidArgument.into());
    }
    
    unsafe {
        // Safety: ポート番号が有効範囲内であることを確認済み
        let mut port_obj = Port::<u8>::new(port);
        port_obj.write(value);
    }
    
    Ok(())
}
```

### MMIO（メモリマップドI/O）

```rust
pub struct MmioRegister {
    addr: usize,
}

impl MmioRegister {
    /// # Safety
    /// 
    /// addr は有効なMMIOアドレスである必要があります
    pub unsafe fn new(addr: usize) -> Self {
        Self { addr }
    }
    
    pub fn read(&self) -> u32 {
        unsafe {
            // Safety: MmioRegister のコンストラクタが unsafe で、
            // 呼び出し側が有効性を保証
            core::ptr::read_volatile(self.addr as *const u32)
        }
    }
    
    pub fn write(&mut self, value: u32) {
        unsafe {
            // Safety: 同上
            core::ptr::write_volatile(self.addr as *mut u32, value);
        }
    }
}
```

## 割り込みハンドラ

### 安全な割り込みハンドラ

```rust
extern "x86-interrupt" fn timer_interrupt_handler(
    _stack_frame: InterruptStackFrame
) {
    // 割り込み中は最小限の処理のみ
    TICKS.fetch_add(1, Ordering::Relaxed);
    
    // PIC に EOI を送信
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(TIMER_INTERRUPT_ID);
    }
}
```

### 割り込み無効化

```rust
pub fn critical_section<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    // 割り込みを無効化
    ArchCpu::disable_interrupts();
    
    let result = f();
    
    // 割り込みを再有効化
    ArchCpu::enable_interrupts();
    
    result
}
```

## チェックリスト

コードレビュー時に確認すべき項目：

### メモリ安全性

- [ ] すべてのポインタがnullチェックされているか
- [ ] アラインメント要件が満たされているか
- [ ] オーバーフローが`checked_*`メソッドで検出されているか
- [ ] メモリリークの可能性がないか
- [ ] 寿命（lifetime）が適切か

### unsafe使用

- [ ] `unsafe`ブロックが最小限か
- [ ] `# Safety`ドキュメントが存在するか
- [ ] 不変条件が文書化されているか
- [ ] `unsafe`操作の前に適切な検証が行われているか

### 並行性

- [ ] グローバル状態が`Mutex`で保護されているか
- [ ] デッドロックの可能性がないか
- [ ] Atomic操作で適切な`Ordering`が使用されているか
- [ ] 非同期タスクでパニックが適切に処理されているか

### ハードウェア

- [ ] I/Oポート番号が有効範囲内か
- [ ] MMIOアドレスが検証されているか
- [ ] volatile操作が適切に使用されているか
- [ ] 割り込みハンドラが再入可能か

## まとめ

- **`unsafe`は最小限** - 必要な箇所のみで使用し、常に検証を行う
- **ドキュメント化** - すべての`unsafe`に`# Safety`セクションを記述
- **ポインタ検証** - null, アラインメント, オーバーフローをチェック
- **並行性の保護** - Mutex と Atomic で共有状態を保護
- **割り込み安全** - クリティカルセクションを適切に保護
- **レビュー必須** - unsafe コードは必ず複数人でレビュー
