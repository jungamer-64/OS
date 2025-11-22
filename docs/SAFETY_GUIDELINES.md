# セーフティガイドライン

## 目的

このドキュメントは、Tiny OSカーネル開発における安全性のベストプラクティスを定義します。メモリ安全性、スレッド安全性、`unsafe`コードの適切な使用方法について説明します。

## Strong Typing（強い型付け）

### 基本原則

**`usize`の直接使用は禁止** - アドレス、サイズ、オフセットなど、すべて専用の型で表現する

### メモリアドレス型

```rust
/// 物理アドレス（型安全性を保証）
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(usize);

impl PhysAddr {
    /// 物理アドレスを作成（検証なし）
    /// 
    /// # Safety
    /// 
    /// 呼び出し元は addr が有効な物理アドレスであることを保証する必要があります
    pub const unsafe fn new_unchecked(addr: usize) -> Self {
        Self(addr)
    }
    
    /// アラインメント検証付きで物理アドレスを作成
    pub fn new_aligned(addr: usize, align: usize) -> Result<Self, MemoryError> {
        if addr % align != 0 {
            return Err(MemoryError::MisalignedAccess);
        }
        Ok(Self(addr))
    }
    
    /// アドレス値を取得
    pub const fn as_usize(&self) -> usize {
        self.0
    }
    
    /// アドレス値をu64として取得
    pub const fn as_u64(&self) -> u64 {
        self.0 as u64
    }
    
    /// 指定されたアラインメントに揃っているか確認
    pub const fn is_aligned(&self, align: usize) -> bool {
        self.0 % align == 0
    }
    
    /// ミュータブルポインタへ変換（Strict Provenance準拠）
    /// 
    /// # Safety
    /// 
    /// - このアドレスが有効なメモリ領域を指していること
    /// - 型Tのアラインメント要件を満たしていること
    /// - 排他的アクセスが保証されていること
    pub unsafe fn as_mut_ptr<T>(&self) -> *mut T {
        core::ptr::from_exposed_addr_mut(self.0)
    }
}

/// 仮想アドレス（型安全性を保証）
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtAddr(usize);

impl VirtAddr {
    /// 仮想アドレスを作成（検証なし）
    /// 
    /// # Safety
    /// 
    /// 呼び出し元は addr が有効な仮想アドレスであることを保証する必要があります
    pub const unsafe fn new_unchecked(addr: usize) -> Self {
        Self(addr)
    }
    
    /// アラインメント検証付きで仮想アドレスを作成
    pub fn new_aligned(addr: usize, align: usize) -> Result<Self, MemoryError> {
        if addr % align != 0 {
            return Err(MemoryError::MisalignedAccess);
        }
        Ok(Self(addr))
    }
    
    /// アドレス値を取得
    pub const fn as_usize(&self) -> usize {
        self.0
    }
    
    /// 指定されたアラインメントに揃っているか確認
    pub const fn is_aligned(&self, align: usize) -> bool {
        self.0 % align == 0
    }
    
    /// ミュータブルポインタへ変換（Strict Provenance準拠）
    /// 
    /// # Safety
    /// 
    /// - このアドレスが有効なメモリ領域を指していること
    /// - 型Tのアラインメント要件を満たしていること
    /// - 排他的アクセスが保証されていること
    pub unsafe fn as_mut_ptr<T>(&self) -> *mut T {
        core::ptr::from_exposed_addr_mut(self.0)
    }
}

/// メモリレイアウトサイズ（型安全性を保証）
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LayoutSize(usize);

impl LayoutSize {
    /// サイズを作成
    pub const fn new(size: usize) -> Self {
        Self(size)
    }
    
    /// 最小サイズ要件を検証
    pub fn new_checked(size: usize, min: usize) -> Result<Self, MemoryError> {
        if size < min {
            return Err(MemoryError::RegionTooSmall);
        }
        Ok(Self(size))
    }
    
    /// サイズ値を取得
    pub const fn as_usize(&self) -> usize {
        self.0
    }
    
    /// 指定されたアラインメントに切り上げ
    pub fn align_up(&self, align: usize) -> Option<Self> {
        if align == 0 || (align & (align - 1)) != 0 {
            return None;
        }
        let mask = align.wrapping_sub(1);
        self.0.checked_add(mask).map(|n| Self(n & !mask))
    }
}

/// ページフレーム番号（型安全性を保証）
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PageFrameNumber(u64);

impl PageFrameNumber {
    pub const fn new(pfn: u64) -> Self {
        Self(pfn)
    }
    
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
    
    /// ページフレームの物理アドレスを計算（4KiBページ想定）
    pub const fn to_phys_addr(&self) -> PhysAddr {
        PhysAddr((self.0 as usize) << 12)
    }
}
```

### ❌ 悪い例（プリミティブ執着）

```rust
// コンパイル時に引数の順序ミスを検出できない
fn add_region(addr: usize, size: usize) { ... }
add_region(0x1000, 0x5000);  // OK
add_region(0x5000, 0x1000);  // バグだがコンパイル通る
```

### ✅ 良い例（型安全）

```rust
// 型が違うため、引数を間違えるとコンパイルエラー
fn add_region(addr: PhysAddr, size: LayoutSize) -> KernelResult<()> { ... }
add_region(PhysAddr::new_unchecked(0x1000), LayoutSize::new(0x5000));  // OK
add_region(LayoutSize::new(0x5000), PhysAddr::new_unchecked(0x1000));  // コンパイルエラー
```

## `unsafe`コードの原則

### 基本ルール

1. **最小限の使用** - `unsafe`は絶対に必要な場合のみ使用
2. **明示的なunsafeブロック** - `#![deny(unsafe_op_in_unsafe_fn)]`を有効化
3. **詳細なドキュメント** - すべての`unsafe`に対して`# Safety`セクションを記述
4. **不変条件の明示** - 何が保証されているかを明確に
5. **型で表現** - `usize`ではなく`PhysAddr`/`VirtAddr`などの専用型を使用

### unsafe関数の書き方

**重要:** 検証可能な関数は `unsafe` を外し、`Result` を返すべきです。

#### ❌ 悪い例（検証しているのにunsafe）

```rust
// assert!で検証しているなら、unsafeである必要がない
unsafe fn add_free_region(&mut self, addr: usize, size: usize) {
    assert_eq!(align_up(addr, ...), Some(addr));
    assert!(size >= mem::size_of::<ListNode>());
    unsafe { /* ... */ }
}
```

#### ✅ 良い例（型安全 + Result）

```rust
/// 指定された領域を空きリストに追加
/// 
/// アラインメント違反やサイズ不足の場合はエラーを返します。
/// 内部で unsafe を使用しますが、検証済みのため呼び出し側は安全です。
pub fn add_free_region(
    &mut self,
    addr: PhysAddr,
    size: LayoutSize
) -> KernelResult<()> {
    // 型安全なアラインメントチェック
    if !addr.is_aligned(mem::align_of::<ListNode>()) {
        return Err(MemoryError::MisalignedAccess.into());
    }
    
    // 型安全なサイズチェック
    if size.as_usize() < mem::size_of::<ListNode>() {
        return Err(MemoryError::RegionTooSmall.into());
    }
    
    // Safety: 上記で検証済み
    // - addr は ListNode のアラインメントを満たしている
    // - size は ListNode を格納できる
    // - &mut self により排他的アクセスが保証されている
    unsafe {
        let node_ptr = addr.as_mut_ptr::<ListNode>();
        node_ptr.write(ListNode::new(size.as_usize()));
        self.head.next = Some(&mut *node_ptr);
    }
    
    Ok(())
}
```

#### パフォーマンス優先の場合（unsafeのまま）

```rust
/// 指定された領域を空きリストに追加（高速版）
/// 
/// # Safety
/// 
/// 呼び出し元は以下を保証する必要があります:
/// - addr は ListNode のアラインメント要件を満たしている
/// - size は少なくとも mem::size_of::<ListNode>() 以上である
/// - addr が指す領域は有効で、他からアクセスされていない
/// - この関数の呼び出し中、他のスレッドが self にアクセスしない
pub unsafe fn add_free_region_unchecked(
    &mut self,
    addr: PhysAddr,
    size: LayoutSize
) {
    // リリースビルドでは検証をスキップ（パフォーマンス優先）
    debug_assert!(addr.is_aligned(mem::align_of::<ListNode>()));
    debug_assert!(size.as_usize() >= mem::size_of::<ListNode>());
    
    unsafe {
        let node_ptr = addr.as_mut_ptr::<ListNode>();
        node_ptr.write(ListNode::new(size.as_usize()));
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

**✅ 良い例（Strict Provenance準拠）:**

```rust
// null チェック
if ptr.is_null() {
    return Err(MemoryError::InvalidAddress.into());
}

// アラインメントチェック（Strict Provenance）
// ❌ 古い方法: if (ptr as usize) % align != 0
// ✅ 新しい方法:
if ptr.addr() % mem::align_of::<T>() != 0 {
    return Err(MemoryError::MisalignedAccess.into());
}

// 範囲チェック（オーバーフロー対策）
let end_addr = ptr.addr()
    .checked_add(mem::size_of::<T>())
    .ok_or(MemoryError::AddressOverflow)?;

if end_addr > MAX_VALID_ADDRESS {
    return Err(MemoryError::OutOfBounds.into());
}

unsafe {
    // Safety: 
    // - ptr が null でないことを確認済み
    // - アラインメント要件を満たしている
    // - 有効なメモリ範囲内である
    *ptr = value;
}
```

## メモリ安全性

### ポインタ操作（Strict Provenance準拠）

**重要:** Rustの最新ガイドラインでは、ポインタと整数の相互変換に制約があります。

- **非推奨:** `ptr as usize` / `addr as *mut T`（来歴情報が失われる）
- **推奨:** `ptr.addr()` / `core::ptr::from_exposed_addr_mut(addr)`

#### 生ポインタの検証（型安全 + Strict Provenance）

```rust
// ✅ モダンで安全な実装
pub fn from_raw_parts(
    addr: PhysAddr,
    len: LayoutSize
) -> KernelResult<&'static mut [u8]> {
    let addr_val = addr.as_usize();
    let len_val = len.as_usize();
    
    // アラインメントチェック（Strict Provenance準拠）
    if addr_val % align_of::<u8>() != 0 {
        return Err(MemoryError::MisalignedAccess.into());
    }
    
    // オーバーフローチェック
    let end = addr_val.checked_add(len_val)
        .ok_or(MemoryError::AddressOverflow)?;
    
    // 有効なメモリ範囲かチェック
    if end > MAX_PHYSICAL_ADDRESS {
        return Err(MemoryError::OutOfBounds.into());
    }
    
    unsafe {
        // Safety: 
        // - アラインメント要件を満たしている
        // - オーバーフローしない
        // - 有効な物理メモリ範囲内
        // - 'static ライフタイムは物理メモリの性質上妥当
        let ptr = addr.as_mut_ptr::<u8>();
        Ok(core::slice::from_raw_parts_mut(ptr, len_val))
    }
}
```

#### ポインタと整数の変換

```rust
// ❌ 非推奨（Strict Provenance違反）
let addr = ptr as usize;
let ptr = addr as *mut T;

// ✅ 推奨（Strict Provenance準拠）
let addr = ptr.addr();  // ポインタのアドレス部分のみ取得
let ptr = core::ptr::from_exposed_addr_mut::<T>(addr);  // アドレスからポインタ生成

// さらに良い: 型安全な抽象化
let phys_addr = PhysAddr::new_aligned(addr, align_of::<T>())?;
let ptr = unsafe { phys_addr.as_mut_ptr::<T>() };
```

#### アライメント要件

型安全な実装では、アラインメント操作は各型のメソッドとして提供されます。

```rust
// ✅ 型安全なアライメント操作
let addr = PhysAddr::new_unchecked(0x1234);
let aligned = addr.as_usize()
    .checked_add(align - 1)
    .map(|a| PhysAddr::new_unchecked(a & !(align - 1)));

// または LayoutSize のメソッドを使用
let size = LayoutSize::new(100);
let aligned_size = size.align_up(16)
    .ok_or(MemoryError::AlignmentError)?;
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

**重要:** カーネル環境（`#![no_std]`）では `std::panic::catch_unwind` は使用できません。

**基本方針:**
1. タスクは `Result` を返すようにし、エラーは伝播させる
2. パニック時はカーネル全体を停止させる（パニックの隔離は高コスト）
3. どうしても隔離が必要な場合は、独自の巻き戻し機構を実装

#### ✅ Result ベースのエラー処理

```rust
// タスクは Result を返す
pub struct Task {
    id: TaskId,
    future: Pin<Box<dyn Future<Output = KernelResult<()>>>>,
}

impl Executor {
    pub fn run(&self) -> KernelResult<()> {
        loop {
            let task_id = match self.task_queue.lock().pop_front() {
                Some(id) => id,
                None => break,
            };
            
            let task = self.tasks.lock()
                .get_mut(&task_id)
                .ok_or(ErrorKind::TaskNotFound)?;
            
            // Futureをポーリング
            let waker = self.create_waker(task_id);
            let mut context = Context::from_waker(&waker);
            
            match task.future.as_mut().poll(&mut context) {
                Poll::Ready(Ok(())) => {
                    // タスク完了
                    self.tasks.lock().remove(&task_id);
                }
                Poll::Ready(Err(e)) => {
                    // タスクがエラーを返した（パニックではない）
                    log::error!("Task {} failed: {:?}", task_id, e);
                    self.tasks.lock().remove(&task_id);
                    
                    // エラーを記録するが、他のタスクは継続
                    self.failed_tasks.lock().push((task_id, e));
                }
                Poll::Pending => {
                    // タスクはまだ完了していない
                }
            }
        }
        
        Ok(())
    }
}
```

#### パニック時の動作

```rust
// カーネルのパニックハンドラ
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // 割り込みを無効化
    ArchCpu::disable_interrupts();
    
    // パニック情報をログ出力
    log::error!("KERNEL PANIC: {}", info);
    
    // スタックトレースを出力（可能なら）
    #[cfg(feature = "backtrace")]
    print_stack_trace();
    
    // システムを停止
    loop {
        ArchCpu::halt();
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

### 割り込み無効化（正しい実装）

**❌ 致命的なバグ（元々無効だった割り込みが有効化されてしまう）:**

```rust
// これは絶対にやってはいけない
pub fn critical_section_WRONG<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    ArchCpu::disable_interrupts();
    let result = f();
    ArchCpu::enable_interrupts();  // ⚠️ 元の状態を無視して強制有効化
    result
}
```

**✅ 正しい実装（割り込みフラグを保存・復元）:**

```rust
/// クリティカルセクションを実行（割り込みフラグを保存・復元）
/// 
/// パニック時でも割り込みフラグが正しく復元されることを保証します。
pub fn critical_section<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    // 現在の割り込みフラグを保存してから無効化
    let saved_flags = ArchCpu::save_and_disable_interrupts();
    
    // パニック時でも復元を保証するRAIIガード
    struct InterruptGuard(InterruptFlags);
    
    impl Drop for InterruptGuard {
        fn drop(&mut self) {
            // スコープを抜ける際に元の状態に戻す
            // パニック時も含めて必ず実行される
            unsafe {
                // Safety: save_and_disable_interrupts で保存した
                // 正当なフラグ値を復元している
                ArchCpu::restore_interrupts(self.0);
            }
        }
    }
    
    let _guard = InterruptGuard(saved_flags);
    
    // クリティカルセクションを実行
    // パニックしても _guard のドロップで割り込みフラグは復元される
    f()
}
```

**実装例（x86_64）:**

```rust
/// 割り込みフラグの状態
#[derive(Clone, Copy)]
pub struct InterruptFlags(u64);

impl ArchCpu {
    /// 現在の割り込みフラグを保存し、割り込みを無効化
    #[inline]
    pub fn save_and_disable_interrupts() -> InterruptFlags {
        let rflags: u64;
        unsafe {
            core::arch::asm!(
                "pushfq",
                "pop {}",
                "cli",
                out(reg) rflags,
                options(nomem, nostack, preserves_flags)
            );
        }
        InterruptFlags(rflags)
    }
    
    /// 保存された割り込みフラグを復元
    /// 
    /// # Safety
    /// 
    /// flags は save_and_disable_interrupts で取得した
    /// 正当な値である必要があります
    #[inline]
    pub unsafe fn restore_interrupts(flags: InterruptFlags) {
        unsafe {
            core::arch::asm!(
                "push {}",
                "popfq",
                in(reg) flags.0,
                options(nomem, nostack)
            );
        }
    }
}
```

## チェックリスト

コードレビュー時に確認すべき項目：

### 型安全性

- [ ] `usize` を直接使わず、`PhysAddr`/`VirtAddr`/`LayoutSize` などの型を使用しているか
- [ ] 引数の順序ミスが型システムで検出できるようになっているか
- [ ] 型変換が明示的で、意図が明確か
- [ ] New Type パターンが適切に適用されているか

### メモリ安全性

- [ ] すべてのポインタがnullチェックされているか
- [ ] アラインメント要件が満たされているか（Strict Provenance準拠）
- [ ] オーバーフローが`checked_*`メソッドで検出されているか
- [ ] メモリリークの可能性がないか
- [ ] 寿命（lifetime）が適切か
- [ ] ポインタと整数の変換が `ptr.addr()` / `from_exposed_addr_mut()` で行われているか

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

### 最重要原則

1. **型安全第一** - `usize` を禁止し、専用の型で表現する
2. **Strict Provenance準拠** - ポインタ操作は `ptr.addr()` / `from_exposed_addr_mut()` を使用
3. **`no_std` 環境** - `std::panic` などの標準ライブラリに依存しない
4. **割り込みフラグ保存** - `critical_section` では必ず元の状態を復元
5. **検証してから unsafe** - 検証済みなら関数を安全にし、`Result` を返す

### 実装チェックリスト

- **`unsafe`は最小限** - 必要な箇所のみで使用し、常に検証を行う
- **型で表現** - アドレス・サイズ・オフセットを `PhysAddr`/`VirtAddr`/`LayoutSize` で型安全に
- **ドキュメント化** - すべての`unsafe`に`# Safety`セクションを記述
- **ポインタ検証** - null, アラインメント, オーバーフローをチェック（Strict Provenance準拠）
- **並行性の保護** - Mutex と Atomic で共有状態を保護
- **割り込み安全** - `critical_section` で割り込みフラグを保存・復元
- **Result ベース** - エラーは `Result` で伝播させ、パニックは最終手段
- **レビュー必須** - unsafe コードは必ず複数人でレビュー

### 禁止事項

- ❌ `usize` でアドレスやサイズを直接扱う
- ❌ `ptr as usize` / `addr as *mut T` による変換
- ❌ `ArchCpu::enable_interrupts()` で無条件に有効化
- ❌ `std::panic::catch_unwind` などの標準ライブラリ依存
- ❌ `assert!` で検証しているのに `unsafe` のまま
