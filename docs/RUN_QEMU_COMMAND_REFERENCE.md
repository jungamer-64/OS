# run_qemu.ps1 - AIエージェント向けリファレンス

tiny_os プロジェクトのビルド・実行スクリプト。**作業ディレクトリは必ず `d:\Rust\OS`**。

---

## ✅ DO (してよいこと)

- 基本ビルド＆実行: `.\run_qemu.ps1`
- フルビルド（userland含む）: `.\run_qemu.ps1 -FullBuild`
- リリースビルド: `.\run_qemu.ps1 -Release`
- ビルドのみ: `.\run_qemu.ps1 -BuildOnly`
- CI/テスト用: `.\run_qemu.ps1 -NoGraphic -Timeout 60`
- コード品質チェック: `.\run_qemu.ps1 -Check -BuildOnly`
- クリーンビルド: `.\run_qemu.ps1 -Clean` → `.\run_qemu.ps1 -FullBuild`
- デバッグ（GDB）: `.\run_qemu.ps1 -Debug`
- ログ確認: `logs/qemu.debug.log`, `logs/qemu.stdout.log`

---

## ❌ DON'T (してはいけないこと)

- **`-Accel` を安易に使わない** → WHPX未対応環境では失敗する
- **`-Timeout 0` で無限待機しない** → CI/自動テストでは必ずタイムアウト設定
- **`-Menu` を自動化で使わない** → インタラクティブ専用
- **リポジトリルート以外で実行しない**
- **存在しないパラメータを渡さない**

---

## パラメータ一覧

### ビルド制御

| パラメータ | 説明 |
|-----------|------|
| `-SkipBuild` | ビルドスキップ（既存成果物使用） |
| `-FullBuild` | userland + kernel をビルド |
| `-Release` | 最適化ビルド |
| `-BuildOnly` | ビルドのみ、QEMU起動なし |
| `-Clean` | 全成果物削除して終了 |
| `-Check` | ビルド前に `cargo clippy` 実行 |

### QEMU実行

| パラメータ | デフォルト | 説明 |
|-----------|-----------|------|
| `-Debug` | `$false` | GDBスタブ有効化（localhost:1234） |
| `-NoGraphic` | `$false` | GUI無し（シリアル→stdout） |
| `-Timeout` | `0` | タイムアウト秒（0=無制限） |
| `-KeepAlive` | `$false` | クラッシュ後もQEMU維持 |

### ハードウェア

| パラメータ | デフォルト | 説明 |
|-----------|-----------|------|
| `-Memory` | `"128M"` | メモリサイズ |
| `-Cores` | `1` | CPUコア数 |
| `-Accel` | `$false` | WHPX加速（⚠️環境依存） |
| `-Network` | `$false` | ネットワーク有効化 |

### その他

| パラメータ | 説明 |
|-----------|------|
| `-ExtraQemuArgStr` | 追加QEMUオプション（文字列） |
| `-QemuPath` | QEMUパス指定 |

---

## よく使うパターン

```powershell
# 開発中（高速）
.\run_qemu.ps1

# フルビルド＋テスト
.\run_qemu.ps1 -FullBuild -Release

# CI向け
.\run_qemu.ps1 -FullBuild -NoGraphic -Timeout 120

# クリーンビルド
.\run_qemu.ps1 -Clean; .\run_qemu.ps1 -FullBuild
```

---

## 出力先

| ファイル | 内容 |
|---------|------|
| `logs/qemu.debug.log` | デバッグ出力 |
| `logs/qemu.stdout.log` | シリアル出力 |
| `logs/history/` | 過去ログ（最大20件） |

---

## 終了コード

- `0`: 成功
- `1`: エラー

---

## 前提条件

- `rustup` が PATH に存在
- QEMU インストール済み
- `ovmf-x64/OVMF.fd` が存在
