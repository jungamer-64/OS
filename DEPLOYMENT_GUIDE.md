# Real Hardware Deployment Guide

## 概要

このガイドでは、Rust OS カーネルを実機にデプロイして実行する手順を説明します。QEMU エミュレーターでの動作確認後、実際のx86_64ハードウェアでの動作確認を行う際に参照してください。

> [!NOTE]
> 現在、実機デプロイは x86_64 アーキテクチャでのみサポートされています。他のアーキテクチャ (AArch64, RISC-V) への移植方法については [docs/PORTING.md](PORTING.md) を参照してください。

## 前提条件

### 必要なもの

- **ビルド済みのカーネルイメージ**

  ```bash
  cargo bootimage
  # 出力: target/x86_64-blog_os/release/bootimage-tiny_os.bin
  ```

- **USBメモリ** (512MB以上推奨)
  - ⚠️ **警告**: USBメモリの全データが消去されます

- **実機** (x86_64アーキテクチャ)
  - Intel/AMD CPU (64bit)
  - 最低 512MB RAM
  - VGA出力またはシリアルポート (COM1) のいずれか

### ソフトウェア要件

```bash
# ddコマンド (Linuxでは標準インストーラ済み)
which dd

# オプション: isoイメージ作成用
sudo apt install xorriso grub-pc-bin  # Ubuntu/Debian
```

## アーキテクチャ選択

現在の実装では x86_64 のみがサポートされています。

```bash
# デフォルト (x86_64)
MAKE build

# 明示的にアーキテクチャを指定
make build ARCH=x86_64

# 将来的なサポート (現在は未実装)
# make build ARCH=aarch64
# make build ARCH=riscv64
```

> [!IMPORTANT]
> ビルドシステムは複数アーキテクチャの基盤が整っていますが、実装されているのは x86_64 のみです。

## デプロイ方法

### 方法1: USBメモリへの直接書き込み (推奨)

この方法が最もシンプルで確実です。

#### 1. USBデバイス名の確認

```bash
# USBメモリを接続する前
lsblk

# USBメモリを接続した後
lsblk

# 差分から /dev/sdX を特定 (例: /dev/sdb)
```

⚠️ **重要**: デバイス名を間違えると、システムドライブを破壊する可能性があります。

#### 2. イメージの書き込み

```bash
# リリースビルドを使用
sudo dd if=target/x86_64-blog_os/release/bootimage-tiny_os.bin \
        of=/dev/sdX \
        bs=1M \
        status=progress \
        conv=fsync

# 書き込み完了後、USBメモリをアンマウント
sudo eject /dev/sdX
```

**パラメータ説明:**

- `if`: 入力ファイル (カーネルイメージ)
- `of`: 出力先デバイス
- `bs=1M`: 1MBブロックサイズで転送 (高速化)
- `status=progress`: 進行状況表示
- `conv=fsync`: 書き込みバッファを確実にフラッシュ

#### 3. 実機での起動

1. USBメモリをターゲットPCに接続
2. BIOS/UEFI設定に入る (通常 F2, F12, Del キー)
3. ブートデバイスをUSBメモリに設定
4. 起動

### 方法2: ISOイメージの作成

CD/DVDブートやVirtual CDとして使用する場合に便利です。

#### ISOイメージ作成手順

```bash
# grubディレクトリ構造を作成
mkdir -p iso/boot/grub

# カーネルイメージをコピー
cp target/x86_64-blog_os/release/bootimage-tiny_os.bin iso/boot/kernel.bin

# grub.cfgを作成
cat > iso/boot/grub/grub.cfg << 'EOF'
set timeout=0
set default=0

menuentry "Rust OS" {
    multiboot2 /boot/kernel.bin
    boot
}
EOF

# ISOイメージを作成
grub-mkrescue -o rust_os.iso iso/
```

#### ISOイメージの使用

```bash
# USBメモリに書き込み
sudo dd if=rust_os.iso of=/dev/sdX bs=4M status=progress conv=fsync

# または、CD-Rに焼く
brasero rust_os.iso  # GUIツール
# または
cdrecord -v dev=/dev/sr0 rust_os.iso  # CLIツール
```

## BIOS/UEFI設定

### Legacy BIOS モード (推奨)

このカーネルはBIOSテキストモード (0xB8000) を前提としています。

#### 必要な設定

1. **Boot Mode**: `Legacy` または `CSM` (Compatibility Support Module) を有効化
2. **Secure Boot**: 無効化 (OSが署名されていないため)
3. **Boot Priority**: USBデバイスを最優先に設定

#### メーカー別の設定例

**Dell:**

- `Boot Mode` → `Legacy`
- `Secure Boot` → `Disabled`

**HP:**

- `Boot Options` → `Legacy Support` → `Enabled`
- `Secure Boot` → `Disabled`

**Lenovo:**

- `Boot` → `Boot Mode` → `Legacy Only`
- `Security` → `Secure Boot` → `Disabled`

**ASUS:**

- `Boot` → `CSM (Compatibility Support Module)` → `Enabled`
- `Secure Boot` → `Disabled`

### UEFI モード

📋 **Framebuffer実装状況**: 基盤モジュール完成（Phase 1-2）

**現在の状態:**

- ✅ Framebufferモジュール実装済み（pixel操作、font rendering）
- ✅ Display抽象化層統合済み
- ⚠️ Bootloader統合は保留中（bootloader 0.11との互換性問題により）

**UEFI環境で動作させる場合:**

1. **CSM有効化（推奨）:**
   - CSM (Compatibility Support Module) を有効化
   - UEFIファームウェアがBIOSエミュレーションを提供
   - VGAテキストモードで完全動作

2. **純粋なUEFI（CSMなし）:**
   - 現在は非対応（画面表示不可の可能性）
   - シリアルポート出力は動作する可能性
   - 将来の拡張: framebufferモジュールを有効化予定

**技術詳細:**

Framebufferサポートモジュールは実装済みです：

- `src/framebuffer/`: ピクセル操作、8x16フォント、RGB色変換
- `src/display/backend.rs`: FramebufferDisplayバックエンド
- Bootloader 0.9との統合方法は今後検討予定

## トラブルシューティング

### 画面が真っ黒 / 何も表示されない

**原因の可能性:**

1. **UEFIモードで起動している**
   - **解決策**: BIOS設定でCSMを有効化

2. **VGAバッファにアクセスできない**
   - **解決策**: シリアルポート経由で確認

3. **カーネルパニック**
   - **解決策**: シリアル接続で詳細を確認

**確認手順:**

```bash
# シリアルポート接続 (別PCから)
screen /dev/ttyUSB0 38400

# または
minicom -D /dev/ttyUSB0 -b 38400
```

### シリアルポートが動作しない

**原因:**

- 最近のマザーボードには物理的なCOM1ポートがない

**解決策:**

- USB-シリアル変換ケーブルを使用
- または、VGA出力のみで運用

**確認方法:**

1. BIOS設定でシリアルポートを確認
   - `I/O Port` が `3F8` (COM1) に設定されているか

2. Linux上でポート確認

   ```bash
   dmesg | grep ttyS
   # ttyS0 が 0x3F8 であることを確認
   ```

### ブートループ / 再起動を繰り返す

**原因:**

- bootloaderの問題
- ハードウェア非互換

**解決策:**

1. **別のUSBポートを試す** (USB 2.0推奨)
2. **BIOS設定をリセット** (Load Defaults)
3. **デバッグビルドで確認**

   ```bash
   cargo bootimage  # デバッグビルド
   # (リリースビルドより情報が多い)
   ```

### カーネルが起動しない

**確認ポイント:**

1. **イメージが正しく書き込まれたか**

   ```bash
   # 書き込んだUSBメモリの先頭を確認
   sudo dd if=/dev/sdX bs=512 count=1 | hexdump -C
   # ブートシグネチャ (0x55AA) が末尾にあることを確認
   ```

2. **ビルドエラーがないか**

   ```bash
   cargo build --release 2>&1 | tee build.log
   cargo bootimage --release 2>&1 | tee bootimage.log
   ```

3. **QEMUで動作確認**

   ```bash
   qemu-system-x86_64 -drive format=raw,file=target/x86_64-blog_os/release/bootimage-tiny_os.bin
   ```

## ハードウェア互換性情報

### 動作確認済みハードウェア

このセクションは、実機テスト結果を記録してください。

| メーカー/モデル | CPU | VGA | Serial | 動作状況 |
|---------------|-----|-----|--------|---------|
| (例) Dell Optiplex | Intel i5 | ✅ | ✅ | 完全動作 |
| (例) Lenovo ThinkPad | AMD Ryzen | ✅ | ❌ | VGAのみ |

### 既知の問題

1. **COM1ポートのない最近のPC**
   - シリアルポート出力は動作しない
   - VGA出力は正常に動作

2. **UEFI-onlyシステム** (CSM非サポート)
   - VGAテキストモードが利用不可
   - 画面出力なし (将来対応予定)

3. **高速CPU** (5GHz以上)
   - タイムアウト値の調整が必要な場合あり
   - 通常は問題なし

## シリアルポート接続

### ハードウェア接続

**必要なケーブル:**

- Null modem cable (クロスケーブル)
- または USB-シリアル変換 + シリアルケーブル

**接続例:**

```
[ターゲットPC COM1] <---> [開発PC USB-Serial]
```

### ソフトウェア設定

#### Linux (minicom)

```bash
# インストール
sudo apt install minicom

# 設定
sudo minicom -s
# Serial Device: /dev/ttyUSB0
# Bps: 38400
# Data/Parity/Stop: 8N1
# Hardware Flow Control: No
# Software Flow Control: No

# 接続
minicom -D /dev/ttyUSB0
```

#### Linux (screen)

```bash
screen /dev/ttyUSB0 38400

# 終了: Ctrl+A, K
```

#### Windows (PuTTY)

```
Connection Type: Serial
Serial Line: COM3 (デバイスマネージャーで確認)
Speed: 38400
Data bits: 8
Stop bits: 1
Parity: None
Flow control: None
```

## パフォーマンス最適化

### リリースビルドの最適化

```toml
# Cargo.toml に既に設定済み
[profile.release]
opt-level = "z"      # サイズ最適化
lto = true           # Link-Time Optimization
strip = true         # デバッグシンボル削除
```

### 起動時間の短縮

現在のタイムアウト値は実機での安定動作を優先していますが、特定のハードウェアでさらに短縮可能:

```rust
// src/serial.rs
const TIMEOUT_ITERATIONS: u32 = 5_000_000; // 10M → 5M
```

⚠️ **注意**: 値を小さくしすぎると、低速なCPUで誤動作する可能性があります。

## 次のステップ

### 機能拡張のアイデア

1. **UEFIフレームバッファ対応**
   - bootloader 0.10+ にアップグレード
   - ピクセルベースの描画実装

2. **複数シリアルポート対応**
   - COM2, COM3, COM4 サポート
   - 自動ポート検出

3. **キーボード入力**
   - PS/2キーボードドライバ
   - 簡易シェル実装

4. **ネットワーク**
   - E1000 NIC ドライバ
   - TCP/IPスタック (smoltcp)

## 参考リンク

- [OSDev Wiki - BIOS](https://wiki.osdev.org/BIOS)
- [OSDev Wiki - UEFI](https://wiki.osdev.org/UEFI)
- [Writing an OS in Rust](https://os.phil-opp.com/)
- [bootloader crate documentation](https://docs.rs/bootloader/)

## サポート

問題が発生した場合:

1. **HARDWARE_COMPATIBILITY.md** を確認
2. **ビルドログ** を確認
3. **QEMUで動作確認** (問題の切り分け)
4. **シリアル出力** でカーネルメッセージを確認

---

**最終更新**: v0.3.0
**対応プラットフォーム**: x86_64 (BIOS/CSM)
