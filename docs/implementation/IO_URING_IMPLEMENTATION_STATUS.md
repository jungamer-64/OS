# io_uring 実装状況レポート

## 概要

このドキュメントは、TinyOSにおけるio_uringスタイルの非同期I/O実装の現状を説明します。

## システムアーキテクチャ

現在、2つの ring-based syscall システムが実装されています：

### 1. io_uring API (メイン・動作中)

| Syscall番号 | 名前 | 説明 |
|-------------|------|------|
| 12 | `io_uring_setup` | io_uringインスタンスの初期化 |
| 13 | `io_uring_enter` | 操作の送信と完了の待機 |
| 14 | `io_uring_register` | バッファの登録 |

**実装ファイル:**

- `crates/kernel/src/kernel/io_uring/` - カーネル側実装
- `crates/libuser/src/async_io.rs` - ユーザー空間API (`AsyncContext`)
- `crates/abi/src/io_uring.rs` - ABI定義

**ユーザー空間API:**

```rust
use libuser::async_io::{AsyncContext, AsyncOp};

let mut ctx = AsyncContext::new()?;
ctx.submit(AsyncOp::nop(user_data))?;
ctx.flush()?;
if let Some(result) = ctx.get_completion() {
    // 完了処理
}
```

### 2. Ring API (将来用・実験的)

| Syscall番号 | 名前 | 説明 |
|-------------|------|------|
| 2000 | `ring_enter` | リングバッファのドアベル |
| 2001 | `ring_register` | バッファ登録 |
| 2002 | `ring_setup` | リングコンテキストのセットアップ |

**実装ファイル:**

- `crates/kernel/src/arch/x86_64/syscall_ring.rs` - カーネル側実装
- `crates/libuser/src/ring_io.rs` - ユーザー空間API (`Ring`)

**特徴:**

- SQPOLLモード対応（カーネルポーリングによるsyscallレス操作）
- 最適化された構造体レイアウト（64バイトSQE、16バイトCQE）
- ドアベルスタイルのsyscall（引数なし）

## メモリレイアウト

### io_uring メモリマッピング

```
ユーザー空間ベースアドレス: 0x200000000000 (USER_IO_URING_BASE)

オフセット     | サイズ | 内容
--------------|--------|------------------
0x0000        | 4 KiB  | SQ Header (RingHeader)
0x1000        | 4 KiB  | CQ Header (RingHeader)
0x2000        | 16 KiB | SQ Entries (256 * 64B)
0x6000        | 4 KiB  | CQ Entries (256 * 16B)
```

### Ring API メモリマッピング

```
ユーザー空間ベースアドレス: 0x0000_1000_0000_0000 (USER_RING_CONTEXT_BASE)

構造: RingContextLayout
- sq_header: リングヘッダー
- cq_header: 完了リングヘッダー
- sq_entries: 送信エントリ配列
- cq_entries: 完了エントリ配列
```

## テスト結果

```
=== io_uring Test (New API) ===

[TEST] Ring::setup() - New API (syscall 2002)
  Calling Ring::setup(false)...
  [SKIP] Ring syscall (2002) test skipped - testing io_uring (12) instead

[TEST] AsyncContext::new()
  Context created successfully
  Has available slots
  [PASS]

[TEST] Single NOP operation
  Submitted NOP
  Flush returned completions
  Got completion
  [PASS]

=== io_uring Tests Complete ===
```

## 今後の作業

### 短期目標

1. [ ] io_uringの追加オペレーション実装（read, write, fsync等）
2. [ ] エラーハンドリングの強化
3. [ ] コンテキストのクリーンアップ（プロセス終了時）

### 中期目標

1. [ ] Ring API (syscall 2000-2002) の完全実装
2. [ ] SQPOLLモードの有効化
3. [ ] `SyscallMode::RingBased` の有効化

### 長期目標

1. [ ] 二つのシステムの統一または明確な分離
2. [ ] パフォーマンスベンチマーク
3. [ ] ドキュメントの完成

## 関連ドキュメント

- [io_uring ABI定義](../../crates/abi/src/io_uring.rs)
- [libuser async_io ガイド](../guides/libuser_guide.md)
- [Ring Separation Design](../design/RING_SEPARATION_DESIGN.md)

## 変更履歴

- 2024-11-28: 初版作成 - io_uring (syscall 12-14) 動作確認
