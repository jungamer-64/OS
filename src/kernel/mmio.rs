// src/kernel/mmio.rs
//! MMIO (Memory Mapped I/O) 抽象化
//!
//! 型安全なメモリマップドレジスタアクセスを提供します。

use core::marker::PhantomData;
use core::ptr;
use crate::kernel::core::{KernelResult, MemoryError};

/// 型安全な MMIO レジスタ
#[repr(transparent)]
pub struct MmioReg<T> {
    addr: usize,
    _phantom: PhantomData<T>,
}

impl<T: Copy> MmioReg<T> {
    /// 新しい MMIO レジスタを作成（アドレス検証なし）
    /// 
    /// # Safety
    /// 
    /// addr は有効な MMIO アドレスである必要があります。
    /// また、適切にアライメントされている必要があります。
    pub const unsafe fn new_unchecked(addr: usize) -> Self {
        Self {
            addr,
            _phantom: PhantomData,
        }
    }
    
    /// 検証付きで MMIO レジスタを作成
    /// 
    /// このメソッドは以下をチェックします：
    /// - ヌルポインタでない
    /// - 適切にアライメントされている
    /// - 最小 MMIO アドレス（0x1000）以上
    pub fn new_checked(addr: usize) -> KernelResult<Self> {
        // ヌルポインタチェック
        if addr == 0 {
            return Err(MemoryError::InvalidAddress.into());
        }
        
        // アライメントチェック
        if addr % core::mem::align_of::<T>() != 0 {
            return Err(MemoryError::MisalignedAccess.into());
        }
        
        // 最小 MMIO アドレス（通常 0x1000 以上）
        if addr < 0x1000 {
            return Err(MemoryError::InvalidAddress.into());
        }
        
        Ok(Self {
            addr,
            _phantom: PhantomData,
        })
    }
    
    /// レジスタから読み取り
    /// 
    /// # Safety
    /// 
    /// このレジスタのアドレスが有効であることを保証する必要があります。
    pub unsafe fn read(&self) -> T {
        unsafe { ptr::read_volatile(self.addr as *const T) }
    }
    
    /// レジスタに書き込み
    /// 
    /// # Safety
    /// 
    /// このレジスタのアドレスが有効であることを保証する必要があります。
    pub unsafe fn write(&mut self, value: T) {
        unsafe { ptr::write_volatile(self.addr as *mut T, value) }
    }
}

/// ビットフィールド操作用のヘルパー trait
/// 
/// 複数の整数型に対応したジェネリック実装。
pub trait BitField: Sized + Copy {
    fn set_bit(&mut self, bit: u32);
    fn clear_bit(&mut self, bit: u32);
    fn is_set(&self, bit: u32) -> bool;
}

/// BitField trait を複数の整数型に一括実装
macro_rules! impl_bitfield {
    ($($t:ty),*) => {
        $(
            impl BitField for $t {
                fn set_bit(&mut self, bit: u32) {
                    *self |= 1 << bit;
                }
                
                fn clear_bit(&mut self, bit: u32) {
                    *self &= !(1 << bit);
                }
                
                fn is_set(&self, bit: u32) -> bool {
                    (*self & (1 << bit)) != 0
                }
            }
        )*
    };
}

// u8, u16, u32, u64, usize に BitField を実装
impl_bitfield!(u8, u16, u32, u64, usize);
