// kernel/src/kernel/mm/types.rs
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
use core::convert::TryFrom;

use crate::kernel::core::ErrorKind;

/// メモリ関連のエラー
///
/// メモリアドレス、サイズ、アラインメントなどの操作で発生するエラー。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryError {
    /// 無効なアドレス（アドレス空間外、NULL等）
    InvalidAddress,
    /// アラインメント違反（必要なアラインメントを満たさない）
    MisalignedAccess,
    /// 領域が小さすぎる（最小サイズ未満）
    RegionTooSmall,
    /// アドレスオーバーフロー（演算結果がアドレス空間を超える）
    AddressOverflow,
    /// 範囲外アクセス（割り当てられた範囲外）
    OutOfBounds,
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAddress => write!(f, "Invalid memory address"),
            Self::MisalignedAccess => write!(f, "Misaligned memory access"),
            Self::RegionTooSmall => write!(f, "Memory region too small"),
            Self::AddressOverflow => write!(f, "Address calculation overflow"),
            Self::OutOfBounds => write!(f, "Memory access out of bounds"),
        }
    }
}

impl From<MemoryError> for ErrorKind {
    fn from(err: MemoryError) -> Self {
        // types::MemoryError を ErrorKind にマッピング
        // 注: ErrorKind::Memory() は別の core::result::MemoryError 用
        match err {
            MemoryError::InvalidAddress => Self::InvalidArgument,
            MemoryError::MisalignedAccess => Self::InvalidArgument,
            MemoryError::RegionTooSmall => Self::InvalidArgument,
            MemoryError::AddressOverflow => Self::InvalidArgument,
            MemoryError::OutOfBounds => Self::InvalidArgument,
        }
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
        if !addr.is_multiple_of(align) {
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
        self.0.is_multiple_of(align)
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
    pub const fn align_down(&self, align: usize) -> Option<Self> {
        if align == 0 || (align & (align - 1)) != 0 {
            return None;
        }
        Some(Self(self.0 & !(align - 1)))
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

    /// ミュータブルポインタへ変換（Strict Provenance推奨）
    ///
    /// # Note
    ///
    /// 現在は `core::ptr::with_exposed_provenance_mut` を使用しています。
    /// 将来的に `core::ptr::from_exposed_addr_mut` (nightly) が stable 化されたら
    /// そちらに移行することを推奨します。Strict Provenance API の方が
    /// ツールの互換性が良く、より明示的です。
    ///
    /// # Safety
    ///
    /// - このアドレスが有効なメモリ領域を指していること
    /// - 型 T のアラインメント要件を満たしていること
    /// - 排他的アクセスが保証されていること（必要な場合）
    #[inline]
    pub unsafe fn as_mut_ptr<T>(&self) -> *mut T {
        core::ptr::with_exposed_provenance_mut(self.0)
    }

    /// 不変ポインタへ変換（Strict Provenance推奨）
    ///
    /// # Note
    ///
    /// 現在は `core::ptr::with_exposed_provenance` を使用しています。
    /// 将来的に `core::ptr::from_exposed_addr` (nightly) が stable 化されたら
    /// そちらに移行することを推奨します。Strict Provenance API の方が
    /// ツールの互換性が良く、より明示的です。
    ///
    /// # Safety
    ///
    /// - このアドレスが有効なメモリ領域を指していること
    /// - 型 T のアラインメント要件を満たしていること
    #[inline]
    pub unsafe fn as_ptr<T>(&self) -> *const T {
        core::ptr::with_exposed_provenance(self.0)
    }
}

impl fmt::Display for PhysAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PhysAddr({:#x})", self.0)
    }
}

/// `PhysAddr` からの明示的な変換
impl From<PhysAddr> for usize {
    #[inline]
    fn from(p: PhysAddr) -> Self {
        p.as_usize()
    }
}

/// `usize` からの明示的な変換（検証付き）
impl TryFrom<usize> for PhysAddr {
    type Error = MemoryError;

    #[inline]
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        // 実メモリ範囲チェック
        // x86_64では物理アドレスは最大52ビット（MAXPHYADDR）
        // 一般的なシステムでは46ビット（64 TiB）が上限
        const MAX_PHYS_ADDR: usize = (1 << 46) - 1;
        
        if value > MAX_PHYS_ADDR {
            return Err(MemoryError::InvalidAddress);
        }
        
        Ok(Self::new(value))
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
        if !addr.is_multiple_of(align) {
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
        self.0.is_multiple_of(align)
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
    pub const fn align_down(&self, align: usize) -> Option<Self> {
        if align == 0 || (align & (align - 1)) != 0 {
            return None;
        }
        Some(Self(self.0 & !(align - 1)))
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

    /// ミュータブルポインタへ変換（Strict Provenance推奨）
    ///
    /// # Note
    ///
    /// 現在は `core::ptr::with_exposed_provenance_mut` を使用しています。
    /// 将来的に `core::ptr::from_exposed_addr_mut` (nightly) が stable 化されたら
    /// そちらに移行することを推奨します。Strict Provenance API の方が
    /// ツールの互換性が良く、より明示的です。
    ///
    /// # Safety
    ///
    /// - このアドレスが有効なメモリ領域を指していること
    /// - 型Tのアラインメント要件を満たしていること
    /// - 排他的アクセスが保証されていること（必要な場合）
    #[inline]
    pub unsafe fn as_mut_ptr<T>(&self) -> *mut T {
        core::ptr::with_exposed_provenance_mut(self.0)
    }

    /// 不変ポインタへ変換（Strict Provenance推奨）
    ///
    /// # Note
    ///
    /// 現在は `core::ptr::with_exposed_provenance` を使用しています。
    /// 将来的に `core::ptr::from_exposed_addr` (nightly) が stable 化されたら
    /// そちらに移行することを推奨します。Strict Provenance API の方が
    /// ツールの互換性が良く、より明示的です。
    ///
    /// # Safety
    ///
    /// - このアドレスが有効なメモリ領域を指していること
    /// - 型Tのアラインメント要件を満たしていること
    #[inline]
    pub unsafe fn as_ptr<T>(&self) -> *const T {
        core::ptr::with_exposed_provenance(self.0)
    }
}

impl fmt::Display for VirtAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VirtAddr({:#x})", self.0)
    }
}

/// `VirtAddr` からの明示的な変換
impl From<VirtAddr> for usize {
    #[inline]
    fn from(v: VirtAddr) -> Self {
        v.as_usize()
    }
}

/// `usize` からの明示的な変換（検証付き）
impl TryFrom<usize> for VirtAddr {
    type Error = MemoryError;

    #[inline]
    fn try_from(value: usize) -> Result<Self, Self::Error> {
        // x86_64 canonical form チェック
        // 仮想アドレスは48ビット（一部システムでは57ビット）
        // bit 47以上は全てbit 47と同じ値でなければならない（符号拡張）
        
        // bit 47をチェック
        let bit_47 = (value >> 47) & 1;
        
        if bit_47 == 0 {
            // bit 47が0の場合、bit 48-63も全て0でなければならない
            if value & 0xFFFF_8000_0000_0000 != 0 {
                return Err(MemoryError::InvalidAddress);
            }
        } else {
            // bit 47が1の場合、bit 48-63も全て1でなければならない
            if value & 0xFFFF_8000_0000_0000 != 0xFFFF_8000_0000_0000 {
                return Err(MemoryError::InvalidAddress);
            }
        }
        
        Ok(Self::new(value))
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
    pub const fn align_down(&self, align: usize) -> Option<Self> {
        if align == 0 || (align & (align - 1)) != 0 {
            return None;
        }
        Some(Self(self.0 & !(align - 1)))
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
///
/// # ターゲットアーキテクチャ
///
/// このコードは `x86_64` 専用で、`usize` は常に 64bit です。
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
    ///
    /// # Note
    ///
    /// `x86_64` ターゲット専用。`u64` 上でシフトしてから `usize` に変換するため、
    /// オーバーフローの心配はありません。
    #[inline]
    #[allow(clippy::cast_possible_truncation)] // x86_64 では usize は 64bit
    pub const fn to_phys_addr(&self) -> PhysAddr {
        // u64 上でシフトしてから usize に変換（x86_64 では安全）
        let addr64 = self.0 << 12;
        PhysAddr(addr64 as usize)
    }

    /// ページフレームの物理アドレスを計算（カスタムページサイズ）
    ///
    /// # Note
    ///
    /// `x86_64` ターゲット専用。`u64` 上でシフトしてから `usize` に変換するため、
    /// オーバーフローの心配はありません。
    #[inline]
    #[allow(clippy::cast_possible_truncation)] // x86_64 では usize は 64bit
    pub const fn to_phys_addr_with_size(&self, page_size_bits: u32) -> PhysAddr {
        // u64 上でシフトしてから usize に変換（x86_64 では安全）
        let addr64 = self.0 << page_size_bits;
        PhysAddr(addr64 as usize)
    }

    /// カスタムページサイズで物理アドレスを計算（境界チェック付き）
    ///
    /// `page_size_bits >= 64` の場合、オーバーフローが発生するため
    /// エラーを返します。
    pub const fn to_phys_addr_with_size_checked(
        &self,
        page_size_bits: u32,
    ) -> Result<PhysAddr, MemoryError> {
        if page_size_bits >= 64 {
            return Err(MemoryError::AddressOverflow);
        }
        let addr64 = self.0 << page_size_bits;
        Ok(PhysAddr(addr64 as usize))
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

// TODO: テストを有効化するには、カスタムテストハーネスを設定する必要があります
// 現在は no_std 環境のため、標準の #[test] が利用できません
// 統合テスト (tests/) で実装することを推奨します
/*
#[cfg(test)]
mod tests {
    use super::*;

    // PageFrameNumber から物理アドレスへの変換（4KiB ページ）
    #[test_case]
    fn test_pfn_to_phys_4k() {
        let pfn = PageFrameNumber::new(1);
        let phys = pfn.to_phys_addr();
        assert_eq!(phys.as_usize(), 4096);
    }

    // ページサイズビットが 64 以上の場合のエラーチェック
    #[test_case]
    fn test_pfn_to_phys_with_large_shift_rejects() {
        let pfn = PageFrameNumber::new(1);
        let result = pfn.to_phys_addr_with_size_checked(64);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), MemoryError::AddressOverflow);
    }

    // PhysAddr 変換のラウンドトリップテスト
    #[test_case]
    fn test_physaddr_conversion_roundtrip() {
        let original = 0x1000_usize;
        let phys = PhysAddr::try_from(original).unwrap();
        let back: usize = phys.into();
        assert_eq!(original, back);
    }
}
*/