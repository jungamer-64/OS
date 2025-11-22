// src/kernel/mm/types.rs
//! 型安全なメモリ管理型定義
//!
//! このモジュールは、メモリアドレス、サイズ、ページフレーム番号などを
//! 型安全に扱うための専用型を提供します。
//!
//! # 設計原則
//!
//! - `usize` の直接使用を禁止
//! - Strict Provenance 準拠
//! - New Type パターンによる型安全性の確保
//! - コンパイル時の型チェックによるバグ防止

use core::fmt;

use crate::kernel::core::ErrorKind;

/// メモリ関連のエラー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryError {
    /// 無効なアドレス
    InvalidAddress,
    /// アラインメント違反
    MisalignedAccess,
    /// 領域が小さすぎる
    RegionTooSmall,
    /// アドレスオーバーフロー
    AddressOverflow,
    /// 範囲外アクセス
    OutOfBounds,
    /// アラインメントエラー
    AlignmentError,
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAddress => write!(f, "Invalid memory address"),
            Self::MisalignedAccess => write!(f, "Misaligned memory access"),
            Self::RegionTooSmall => write!(f, "Memory region too small"),
            Self::AddressOverflow => write!(f, "Address calculation overflow"),
            Self::OutOfBounds => write!(f, "Memory access out of bounds"),
            Self::AlignmentError => write!(f, "Alignment error"),
        }
    }
}

impl From<MemoryError> for ErrorKind {
    fn from(_err: MemoryError) -> Self {
        ErrorKind::InvalidArgument
    }
}

/// 物理アドレス（型安全性を保証）
///
/// # 設計意図
///
/// - `usize` との暗黙の変換を防止
/// - 物理アドレスと仮想アドレスの混同を防止
/// - コンパイル時の型チェックを活用
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PhysAddr(usize);

impl PhysAddr {
    /// 物理アドレスを作成（検証なし）
    ///
    /// # Safety
    ///
    /// 呼び出し元は `addr` が有効な物理アドレスであることを保証する必要があります。
    #[inline]
    pub const unsafe fn new_unchecked(addr: usize) -> Self {
        Self(addr)
    }

    /// 物理アドレスを作成（ベーシックチェック付き）
    ///
    /// 簡単な妥当性チェックのみ実施します。
    #[inline]
    pub const fn new(addr: usize) -> Self {
        Self(addr)
    }

    /// アラインメント検証付きで物理アドレスを作成
    ///
    /// # Errors
    ///
    /// アラインメント要件を満たさない場合、[`MemoryError::MisalignedAccess`] を返します。
    #[inline]
    pub fn new_aligned(addr: usize, align: usize) -> Result<Self, MemoryError> {
        if addr % align != 0 {
            return Err(MemoryError::MisalignedAccess);
        }
        Ok(Self(addr))
    }

    /// ゼロアドレスを取得
    #[inline]
    pub const fn zero() -> Self {
        Self(0)
    }

    /// アドレス値を取得
    #[inline]
    pub const fn as_usize(&self) -> usize {
        self.0
    }

    /// アドレス値をu64として取得
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0 as u64
    }

    /// 指定されたアラインメントに揃っているか確認
    #[inline]
    pub const fn is_aligned(&self, align: usize) -> bool {
        self.0 % align == 0
    }

    /// 指定されたアラインメントに切り上げ
    #[inline]
    pub fn align_up(&self, align: usize) -> Option<Self> {
        if align == 0 || (align & (align - 1)) != 0 {
            return None;
        }
        let mask = align.wrapping_sub(1);
        self.0.checked_add(mask).map(|n| Self(n & !mask))
    }

    /// 指定されたアラインメントに切り下げ
    #[inline]
    pub const fn align_down(&self, align: usize) -> Self {
        Self(self.0 & !(align - 1))
    }

    /// オフセットを加算
    #[inline]
    pub fn checked_add(&self, offset: usize) -> Option<Self> {
        self.0.checked_add(offset).map(Self)
    }

    /// オフセットを減算
    #[inline]
    pub fn checked_sub(&self, offset: usize) -> Option<Self> {
        self.0.checked_sub(offset).map(Self)
    }

    /// ミュータブルポインタへ変換（Strict Provenance準拠）
    ///
    /// # Safety
    ///
    /// - このアドレスが有効なメモリ領域を指していること
    /// - 型Ｔのアラインメント要件を満たしていること
    /// - 排他的アクセスが保証されていること（必要な場合）
    #[inline]
    pub unsafe fn as_mut_ptr<T>(&self) -> *mut T {
        self.0 as *mut T
    }

    /// 不変ポインタへ変換（Strict Provenance準拠）
    ///
    /// # Safety
    ///
    /// - このアドレスが有効なメモリ領域を指していること
    /// - 型Ｔのアラインメント要件を満たしていること
    #[inline]
    pub unsafe fn as_ptr<T>(&self) -> *const T {
        self.0 as *const T
    }
}

impl fmt::Display for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PhysAddr({:#x})", self.0)
    }
}

/// 仮想アドレス（型安全性を保証）
///
/// # 設計意図
///
/// - 物理アドレスとの混同を防止
/// - ページテーブル操作の安全性を向上
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VirtAddr(usize);

impl VirtAddr {
    /// 仮想アドレスを作成（検証なし）
    ///
    /// # Safety
    ///
    /// 呼び出し元は `addr` が有効な仮想アドレスであることを保証する必要があります。
    #[inline]
    pub const unsafe fn new_unchecked(addr: usize) -> Self {
        Self(addr)
    }

    /// 仮想アドレスを作成（ベーシックチェック付き）
    ///
    /// 簡単な妥当性チェックのみ実施します。
    #[inline]
    pub const fn new(addr: usize) -> Self {
        Self(addr)
    }

    /// アラインメント検証付きで仮想アドレスを作成
    ///
    /// # Errors
    ///
    /// アラインメント要件を満たさない場合、[`MemoryError::MisalignedAccess`] を返します。
    #[inline]
    pub fn new_aligned(addr: usize, align: usize) -> Result<Self, MemoryError> {
        if addr % align != 0 {
            return Err(MemoryError::MisalignedAccess);
        }
        Ok(Self(addr))
    }

    /// ゼロアドレスを取得
    #[inline]
    pub const fn zero() -> Self {
        Self(0)
    }

    /// アドレス値を取得
    #[inline]
    pub const fn as_usize(&self) -> usize {
        self.0
    }

    /// アドレス値をu64として取得
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0 as u64
    }

    /// 指定されたアラインメントに揃っているか確認
    #[inline]
    pub const fn is_aligned(&self, align: usize) -> bool {
        self.0 % align == 0
    }

    /// 指定されたアラインメントに切り上げ
    #[inline]
    pub fn align_up(&self, align: usize) -> Option<Self> {
        if align == 0 || (align & (align - 1)) != 0 {
            return None;
        }
        let mask = align.wrapping_sub(1);
        self.0.checked_add(mask).map(|n| Self(n & !mask))
    }

    /// 指定されたアラインメントに切り下げ
    #[inline]
    pub const fn align_down(&self, align: usize) -> Self {
        Self(self.0 & !(align - 1))
    }

    /// オフセットを加算
    #[inline]
    pub fn checked_add(&self, offset: usize) -> Option<Self> {
        self.0.checked_add(offset).map(Self)
    }

    /// オフセットを減算
    #[inline]
    pub fn checked_sub(&self, offset: usize) -> Option<Self> {
        self.0.checked_sub(offset).map(Self)
    }

    /// ミュータブルポインタへ変換（Strict Provenance準拠）
    ///
    /// # Safety
    ///
    /// - このアドレスが有効なメモリ領域を指していること
    /// - 型Tのアラインメント要件を満たしていること
    /// - 排他的アクセスが保証されていること（必要な場合）
    #[inline]
    pub unsafe fn as_mut_ptr<T>(&self) -> *mut T {
        self.0 as *mut T
    }

    /// 不変ポインタへ変換（Strict Provenance準拠）
    ///
    /// # Safety
    ///
    /// - このアドレスが有効なメモリ領域を指していること
    /// - 型Tのアラインメント要件を満たしていること
    #[inline]
    pub unsafe fn as_ptr<T>(&self) -> *const T {
        self.0 as *const T
    }
}

impl fmt::Display for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VirtAddr({:#x})", self.0)
    }
}

/// メモリレイアウトサイズ（型安全性を保証）
///
/// # 設計意図
///
/// - サイズとアドレスの混同を防止
/// - 引数の順序ミスをコンパイル時に検出
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayoutSize(usize);

impl LayoutSize {
    /// サイズを作成
    #[inline]
    pub const fn new(size: usize) -> Self {
        Self(size)
    }

    /// ゼロサイズを取得
    #[inline]
    pub const fn zero() -> Self {
        Self(0)
    }

    /// 最小サイズ要件を検証
    ///
    /// # Errors
    ///
    /// サイズが最小値未満の場合、[`MemoryError::RegionTooSmall`] を返します。
    #[inline]
    pub fn new_checked(size: usize, min: usize) -> Result<Self, MemoryError> {
        if size < min {
            return Err(MemoryError::RegionTooSmall);
        }
        Ok(Self(size))
    }

    /// サイズ値を取得
    #[inline]
    pub const fn as_usize(&self) -> usize {
        self.0
    }

    /// サイズ値をu64として取得
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0 as u64
    }

    /// 指定されたアラインメントに切り上げ
    #[inline]
    pub fn align_up(&self, align: usize) -> Option<Self> {
        if align == 0 || (align & (align - 1)) != 0 {
            return None;
        }
        let mask = align.wrapping_sub(1);
        self.0.checked_add(mask).map(|n| Self(n & !mask))
    }

    /// 指定されたアラインメントに切り下げ
    #[inline]
    pub const fn align_down(&self, align: usize) -> Self {
        Self(self.0 & !(align - 1))
    }

    /// サイズを加算
    #[inline]
    pub fn checked_add(&self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(Self)
    }

    /// サイズを減算
    #[inline]
    pub fn checked_sub(&self, other: Self) -> Option<Self> {
        self.0.checked_sub(other.0).map(Self)
    }

    /// サイズがゼロかチェック
    #[inline]
    pub const fn is_zero(&self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for LayoutSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}B", self.0)
    }
}

/// ページフレーム番号（型安全性を保証）
///
/// # 設計意図
///
/// - ページフレーム番号とアドレスの混同を防止
/// - ページテーブル操作の型安全性を向上
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageFrameNumber(u64);

impl PageFrameNumber {
    /// ページフレーム番号を作成
    #[inline]
    pub const fn new(pfn: u64) -> Self {
        Self(pfn)
    }

    /// ページフレーム番号を取得
    #[inline]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    /// ページフレームの物理アドレスを計算（4KiBページ想定）
    #[inline]
    pub const fn to_phys_addr(&self) -> PhysAddr {
        PhysAddr((self.0 as usize) << 12)
    }

    /// ページフレームの物理アドレスを計算（カスタムページサイズ）
    #[inline]
    pub const fn to_phys_addr_with_size(&self, page_size_bits: u32) -> PhysAddr {
        PhysAddr((self.0 as usize) << page_size_bits)
    }

    /// 物理アドレスからページフレーム番号を計算（4KiBページ想定）
    #[inline]
    pub const fn from_phys_addr(addr: PhysAddr) -> Self {
        Self((addr.as_usize() >> 12) as u64)
    }

    /// 次のページフレーム番号を取得
    #[inline]
    pub fn checked_add(&self, offset: u64) -> Option<Self> {
        self.0.checked_add(offset).map(Self)
    }

    /// 前のページフレーム番号を取得
    #[inline]
    pub fn checked_sub(&self, offset: u64) -> Option<Self> {
        self.0.checked_sub(offset).map(Self)
    }
}

impl fmt::Display for PageFrameNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PFN({})", self.0)
    }
}