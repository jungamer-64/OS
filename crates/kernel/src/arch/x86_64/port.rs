// kernel/src/arch/x86_64/port.rs
//! `x86_64` ポート I/O 抽象化
//!
//! 型安全な I/O ポートアクセスを提供します。
//! unsafe 操作を最小限の範囲に閉じ込めます。

use core::marker::PhantomData;

/// ポート番号の有効範囲
/// 
/// x86_64では0x0000-0xFFFFの範囲が有効。
/// ただし、一部のポート（0x0000-0x00FF）は特権ポートとして扱われる。
const MAX_PORT: u16 = 0xFFFF;

/// ポート番号が有効な範囲内かチェック
/// 
/// 実際には u16 なので常に有効範囲内だが、将来の拡張性のために関数として提供。
#[allow(clippy::absurd_extreme_comparisons)]
#[inline]
const fn is_valid_port(port: u16) -> bool {
    port <= MAX_PORT
}

/// 読み書き可能な I/O ポート
#[derive(Debug)]
pub struct Port<T> {
    port: u16,
    _phantom: PhantomData<T>,
}

impl<T> Port<T> {
    /// 新しいポートを作成（const 関数）
    /// 
    /// # Safety
    /// 
    /// ポート番号の妥当性チェックはコンパイル時には行えないため、
    /// 呼び出し元がポート番号の有効性を保証する必要があります。
    /// 
    /// # Panics
    /// 
    /// ポート番号が無効な場合（理論上はu16なので発生しない）
    #[must_use]
    pub const fn new(port: u16) -> Self {
        // コンパイル時定数アサーション（u16なので実際には常に真）
        assert!(is_valid_port(port), "Invalid port number");
        Self {
            port,
            _phantom: PhantomData,
        }
    }
    
    /// ポート番号を取得
    #[must_use]
    pub const fn port_number(&self) -> u16 {
        self.port
    }
}

/// 8ビットポート実装
impl Port<u8> {
    /// ポートから1バイト読み取り
    ///
    /// # Safety
    /// 
    /// 呼び出し元は、指定されたポート番号が有効であり、
    /// 読み取り操作が安全であることを保証する必要があります。
    #[must_use]
    pub unsafe fn read(&self) -> u8 {
        let value: u8;
        unsafe {
            core::arch::asm!(
                "in al, dx",
                in("dx") self.port,
                out("al") value,
                options(nomem, nostack, preserves_flags)
            );
        }
        value
    }

    /// ポートに1バイト書き込み
    ///
    /// # Safety
    /// 
    /// 呼び出し元は、指定されたポート番号が有効であり、
    /// 書き込み操作が安全であることを保証する必要があります。
    pub unsafe fn write(&mut self, value: u8) {
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") self.port,
                in("al") value,
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

/// 16ビットポート実装
impl Port<u16> {
    /// ポートから2バイト読み取り
    ///
    /// # Safety
    /// 
    /// 呼び出し元は、指定されたポート番号が有効であり、
    /// 読み取り操作が安全であることを保証する必要があります。
    #[must_use]
    pub unsafe fn read(&self) -> u16 {
        let value: u16;
        unsafe {
            core::arch::asm!(
                "in ax, dx",
                in("dx") self.port,
                out("ax") value,
                options(nomem, nostack, preserves_flags)
            );
        }
        value
    }

    /// ポートに2バイト書き込み
    ///
    /// # Safety
    /// 
    /// 呼び出し元は、指定されたポート番号が有効であり、
    /// 書き込み操作が安全であることを保証する必要があります。
    pub unsafe fn write(&mut self, value: u16) {
        unsafe {
            core::arch::asm!(
                "out dx, ax",
                in("dx") self.port,
                in("ax") value,
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

/// 32ビットポート実装
impl Port<u32> {
    /// ポートから4バイト読み取り
    ///
    /// # Safety
    /// 
    /// 呼び出し元は、指定されたポート番号が有効であり、
    /// 読み取り操作が安全であることを保証する必要があります。
    #[must_use]
    pub unsafe fn read(&self) -> u32 {
        let value: u32;
        unsafe {
            core::arch::asm!(
                "in eax, dx",
                in("dx") self.port,
                out("eax") value,
                options(nomem, nostack, preserves_flags)
            );
        }
        value
    }

    /// ポートに4バイト書き込み
    ///
    /// # Safety
    /// 
    /// 呼び出し元は、指定されたポート番号が有効であり、
    /// 書き込み操作が安全であることを保証する必要があります。
    pub unsafe fn write(&mut self, value: u32) {
        unsafe {
            core::arch::asm!(
                "out dx, eax",
                in("dx") self.port,
                in("eax") value,
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

/// 読み取り専用 I/O ポート
#[derive(Debug)]
pub struct PortReadOnly<T> {
    port: u16,
    _phantom: PhantomData<T>,
}

impl<T> PortReadOnly<T> {
    /// 新しい読み取り専用ポートを作成
    #[must_use]
    pub const fn new(port: u16) -> Self {
        Self {
            port,
            _phantom: PhantomData,
        }
    }
}

/// 8ビット読み取り専用ポート
impl PortReadOnly<u8> {
    /// ポートから1バイト読み取り
    ///
    /// # Safety
    /// 
    /// 呼び出し元は、指定されたポート番号が有効であり、
    /// 読み取り操作が安全であることを保証する必要があります。
    #[must_use]
    pub unsafe fn read(&self) -> u8 {
        let value: u8;
        unsafe {
            core::arch::asm!(
                "in al, dx",
                in("dx") self.port,
                out("al") value,
                options(nomem, nostack, preserves_flags)
            );
        }
        value
    }
}

/// 16ビット読み取り専用ポート
impl PortReadOnly<u16> {
    /// ポートから2バイト読み取り
    ///
    /// # Safety
    /// 
    /// 呼び出し元は、指定されたポート番号が有効であり、
    /// 読み取り操作が安全であることを保証する必要があります。
    #[must_use]
    pub unsafe fn read(&self) -> u16 {
        let value: u16;
        unsafe {
            core::arch::asm!(
                "in ax, dx",
                in("dx") self.port,
                out("ax") value,
                options(nomem, nostack, preserves_flags)
            );
        }
        value
    }
}

/// 32ビット読み取り専用ポート
impl PortReadOnly<u32> {
    /// ポートから4バイト読み取り
    ///
    /// # Safety
    /// 
    /// 呼び出し元は、指定されたポート番号が有効であり、
    /// 読み取り操作が安全であることを保証する必要があります。
    #[must_use]
    pub unsafe fn read(&self) -> u32 {
        let value: u32;
        unsafe {
            core::arch::asm!(
                "in eax, dx",
                in("dx") self.port,
                out("eax") value,
                options(nomem, nostack, preserves_flags)
            );
        }
        value
    }
}

/// 書き込み専用 I/O ポート
#[derive(Debug)]
pub struct PortWriteOnly<T> {
    port: u16,
    _phantom: PhantomData<T>,
}

impl<T> PortWriteOnly<T> {
    /// 新しい書き込み専用ポートを作成
    #[must_use]
    pub const fn new(port: u16) -> Self {
        Self {
            port,
            _phantom: PhantomData,
        }
    }
}

/// 8ビット書き込み専用ポート
impl PortWriteOnly<u8> {
    /// ポートに1バイト書き込み
    ///
    /// # Safety
    /// 
    /// 呼び出し元は、指定されたポート番号が有効であり、
    /// 書き込み操作が安全であることを保証する必要があります。
    pub unsafe fn write(&mut self, value: u8) {
        unsafe {
            core::arch::asm!(
                "out dx, al",
                in("dx") self.port,
                in("al") value,
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

/// 16ビット書き込み専用ポート
impl PortWriteOnly<u16> {
    /// ポートに2バイト書き込み
    ///
    /// # Safety
    /// 
    /// 呼び出し元は、指定されたポート番号が有効であり、
    /// 書き込み操作が安全であることを保証する必要があります。
    pub unsafe fn write(&mut self, value: u16) {
        unsafe {
            core::arch::asm!(
                "out dx, ax",
                in("dx") self.port,
                in("ax") value,
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

/// 32ビット書き込み専用ポート
impl PortWriteOnly<u32> {
    /// ポートに4バイト書き込み
    ///
    /// # Safety
    /// 
    /// 呼び出し元は、指定されたポート番号が有効であり、
    /// 書き込み操作が安全であることを保証する必要があります。
    pub unsafe fn write(&mut self, value: u32) {
        unsafe {
            core::arch::asm!(
                "out dx, eax",
                in("dx") self.port,
                in("eax") value,
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}
