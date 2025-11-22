// src/kernel/mmio.rs
//! MMIO (Memory Mapped I/O) 抽象化
//!
//! 型安全なメモリマップドレジスタアクセスを提供します。

use core::marker::PhantomData;
use core::ptr::{read_volatile, write_volatile};
use crate::kernel::core::{KernelResult, KernelError, ErrorKind, MemoryError};

/// MMIO レジスタ
#[derive(Debug)]
pub struct MmioReg<T> {
    address: usize,
    _phantom: PhantomData<T>,
}

impl<T> MmioReg<T> {
    /// 新しい MMIO レジスタを作成（未チェック）
    ///
    /// # Safety
    ///
    /// アドレスが有効な MMIO 領域を指していることを保証する必要があります。
    pub const unsafe fn new_unchecked(address: usize) -> Self {
        Self {
            address,
            _phantom: PhantomData,
        }
    }

    /// 新しい MMIO レジスタを作成（チェック付き）
    pub fn new_checked(address: usize) -> KernelResult<Self> {
        if address == 0 {
            return Err(KernelError::with_context(
                ErrorKind::Memory(MemoryError::InvalidAddress),
                "MMIO address cannot be null",
            ));
        }
        if address < 0x1000 {
            return Err(KernelError::with_context(
                ErrorKind::Memory(MemoryError::InvalidAddress),
                "MMIO address too low",
            ));
        }
        if address % core::mem::align_of::<T>() != 0 {
            return Err(KernelError::with_context(
                ErrorKind::Memory(MemoryError::MisalignedAccess),
                "MMIO address misalignment",
            ));
        }
        Ok(unsafe { Self::new_unchecked(address) })
    }

    /// レジスタから値を読み取り
    pub fn read(&self) -> T
    where
        T: Copy,
    {
        unsafe { read_volatile(self.address as *const T) }
    }

    /// レジスタに値を書き込み
    pub fn write(&mut self, value: T)
    where
        T: Copy,
    {
        unsafe { write_volatile(self.address as *mut T, value) }
    }
}

/// ビットフィールド操作 trait
pub trait BitField {
    /// ビットがセットされているか確認
    fn get_bit(&self, bit: u8) -> bool;
    
    /// ビットをセット
    fn set_bit(&mut self, bit: u8, value: bool);
    
    /// ビット範囲を取得
    fn get_bits(&self, range: core::ops::Range<u8>) -> Self;
    
    /// ビット範囲を設定
    fn set_bits(&mut self, range: core::ops::Range<u8>, value: Self);
}

// マクロで各整数型に BitField を実装
macro_rules! impl_bitfield {
    ($($t:ty),*) => {
        $(
            impl BitField for $t {
                fn get_bit(&self, bit: u8) -> bool {
                    (*self & (1 << bit)) != 0
                }

                fn set_bit(&mut self, bit: u8, value: bool) {
                    if value {
                        *self |= 1 << bit;
                    } else {
                        *self &= !(1 << bit);
                    }
                }

                fn get_bits(&self, range: core::ops::Range<u8>) -> Self {
                    let mask = !0 as $t;
                    let shift = range.start;
                    let len = range.end - range.start;
                    let mask = (mask >> (core::mem::size_of::<$t>() as u8 * 8 - len)) << shift;
                    (*self & mask) >> shift
                }

                fn set_bits(&mut self, range: core::ops::Range<u8>, value: Self) {
                    let mask = !0 as $t;
                    let shift = range.start;
                    let len = range.end - range.start;
                    let mask = (mask >> (core::mem::size_of::<$t>() as u8 * 8 - len)) << shift;
                    *self = (*self & !mask) | ((value << shift) & mask);
                }
            }
        )*
    };
}

impl_bitfield!(u8, u16, u32, u64, usize);
