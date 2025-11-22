//! Programmable Interrupt Controller (8259 PIC)
//!
//! 割り込みコントローラの初期化と管理を行います。
//! 標準的なデュアルPIC構成（Master/Slave）をサポートします。

use crate::arch::x86_64::port::{Port, PortWriteOnly};
use spin::Mutex;

/// Master PIC のコマンドポート
const PIC1_COMMAND: u16 = 0x20;
/// Master PIC のデータポート
const PIC1_DATA: u16 = 0x21;
/// Slave PIC のコマンドポート
const PIC2_COMMAND: u16 = 0xA0;
/// Slave PIC のデータポート
const PIC2_DATA: u16 = 0xA1;

/// 初期化コマンド (ICW1)
const ICW1_INIT: u8 = 0x11;
/// 8086/88 モード (ICW4)
const ICW4_8086: u8 = 0x01;
/// End of Interrupt (EOI) コマンド
const PIC_EOI: u8 = 0x20;

/// チェーン接続された PIC
pub struct ChainedPics {
    pics: [Pic; 2],
}

impl ChainedPics {
    /// 指定されたオフセットで新しい PIC チェーンを作成
    #[must_use]
    pub const fn new(offset1: u8, offset2: u8) -> Self {
        Self {
            pics: [
                Pic {
                    offset: offset1,
                    command: PortWriteOnly::new(PIC1_COMMAND),
                    data: PortWriteOnly::new(PIC1_DATA),
                },
                Pic {
                    offset: offset2,
                    command: PortWriteOnly::new(PIC2_COMMAND),
                    data: PortWriteOnly::new(PIC2_DATA),
                },
            ],
        }
    }

    /// PIC を初期化
    ///
    /// # Safety
    /// 
    /// この関数は一度だけ呼ばれる必要があり、他のPIC操作の前に実行される必要があります。
    pub unsafe fn initialize(&mut self) {
        // SAFETY: 呼び出し元がPIC初期化のタイミングを保証している
        unsafe {
            // マスクを保存（現在はすべて無効化するため省略可能だが、念のため）
            // let mut wait_port: Port<u8> = Port::new(0x80);
            // let mut wait = || unsafe { wait_port.write(0) };

            let mut wait_port: PortWriteOnly<u8> = PortWriteOnly::new(0x80);
            let mut wait = || wait_port.write(0);

            // ICW1: 初期化開始
            self.pics[0].command.write(ICW1_INIT);
            wait();
            self.pics[1].command.write(ICW1_INIT);
            wait();

            // ICW2: ベクタオフセット設定
            self.pics[0].data.write(self.pics[0].offset);
            wait();
            self.pics[1].data.write(self.pics[1].offset);
            wait();

            // ICW3: Master/Slave 接続設定
            self.pics[0].data.write(4); // Master: Slave は IRQ2 に接続
            wait();
            self.pics[1].data.write(2); // Slave: 自身のカスケード ID
            wait();

            // ICW4: モード設定 (8086)
            self.pics[0].data.write(ICW4_8086);
            wait();
            self.pics[1].data.write(ICW4_8086);
            wait();

            // マスクをクリア（すべての割り込みを有効化、ただし後で個別にマスク可能）
            // ここではとりあえずすべて無効化（0xFF）せず、すべて有効化（0x00）しておく
            // 実際には必要なものだけ有効にするのが良いが、IDT側でハンドラがないとダブルフォールトになる
            // 安全のため、初期化直後はすべてマスクし、後で個別に解除するのが一般的
            self.pics[0].data.write(0xfb); // IRQ2 (Slave) 以外マスク
            self.pics[1].data.write(0xff); // Slave はすべてマスク
        }
    }

    /// 割り込み終了を通知 (EOI)
    ///
    /// # Safety
    /// 
    /// この関数は有効な割り込みコンテキスト内で、対応する割り込みIDで呼ばれる必要があります。
    pub unsafe fn notify_end_of_interrupt(&mut self, interrupt_id: u8) {
        // SAFETY: 呼び出し元が適切な割り込みコンテキストであることを保証している
        unsafe {
            if self.handles_interrupt(interrupt_id) {
                // Slave PIC からの割り込みなら、Slave にも EOI を送る
                if self.pics[1].handles_interrupt(interrupt_id) {
                    self.pics[1].end_of_interrupt();
                }
                // Master には常に EOI を送る
                self.pics[0].end_of_interrupt();
            }
        }
    }

    fn handles_interrupt(&self, interrupt_id: u8) -> bool {
        self.pics.iter().any(|p| p.handles_interrupt(interrupt_id))
    }
    
    /// 特定の IRQ のマスクを解除
    ///
    /// # Safety
    /// 
    /// この関数はPICが適切に初期化された後に呼ばれる必要があります。
    pub unsafe fn unmask_irq(&mut self, irq: u8) {
        // SAFETY: 呼び出し元がPICマスク操作の安全性を保証している
        unsafe {
            let mut port: Port<u8>;
            if irq < 8 {
                port = Port::new(PIC1_DATA);
                let value = port.read();
                port.write(value & !(1 << irq));
            } else {
                port = Port::new(PIC2_DATA);
                let value = port.read();
                port.write(value & !(1 << (irq - 8)));
            }
        }
    }
}

struct Pic {
    offset: u8,
    command: PortWriteOnly<u8>,
    data: PortWriteOnly<u8>,
}

impl Pic {
    const fn handles_interrupt(&self, interrupt_id: u8) -> bool {
        self.offset <= interrupt_id && interrupt_id < self.offset + 8
    }

    unsafe fn end_of_interrupt(&mut self) {
        // SAFETY: 呼び出し元がEOI送信の安全性を保証している
        unsafe {
            self.command.write(PIC_EOI);
        }
    }
}

// グローバル PIC インスタンス
// Master: 32 (0x20), Slave: 40 (0x28)
pub static PICS: Mutex<ChainedPics> = Mutex::new(ChainedPics::new(0x20, 0x28));
