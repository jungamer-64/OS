# シリアルポートタイムアウト機能の使用方法

このドキュメントでは、シリアルポートモジュールで実装された高度なタイムアウト機能の使用方法を説明します。

## 概要

`src/serial/timeout.rs`モジュールは、以下の機能を提供します：

1. **基本的なタイムアウト機能** - ポーリング操作のタイムアウト処理
2. **リトライ機能** - 失敗した操作の自動リトライ
3. **アダプティブタイムアウト** - 成功率に基づく動的なタイムアウト調整

## 基本的なタイムアウト機能

### TimeoutConfig

タイムアウトの設定を定義します。

```rust
use crate::serial::{TimeoutConfig, poll_with_timeout};

// デフォルト設定（バランス型）
let config = TimeoutConfig::default_timeout();

// 短いタイムアウト（高速操作用）
let config = TimeoutConfig::short_timeout();

// 長いタイムアウト（遅いハードウェア用）
let config = TimeoutConfig::long_timeout();

// カスタム設定
let config = TimeoutConfig {
    max_iterations: 5000,
    backoff: BackoffStrategy::Exponential { base: 2, max: 100 },
};
```

### poll_with_timeout

条件が満たされるまでポーリングします。

```rust
use crate::serial::{poll_with_timeout, TimeoutConfig, TimeoutResult};

let result = poll_with_timeout(
    TimeoutConfig::default_timeout(),
    || {
        // ハードウェアの準備状態をチェック
        hardware_ready()
    }
);

match result {
    TimeoutResult::Ok(()) => {
        // 成功
        println!("Hardware is ready!");
    }
    TimeoutResult::Timeout { iterations, total_waits } => {
        // タイムアウト
        println!("Timeout after {} iterations", iterations);
    }
}
```

### poll_with_timeout_value

値を返すポーリング操作。

```rust
use crate::serial::{poll_with_timeout_value, TimeoutConfig};

let result = poll_with_timeout_value(
    TimeoutConfig::default_timeout(),
    || {
        // データが利用可能な場合はSome(value)を返す
        if data_available() {
            Some(read_data())
        } else {
            None
        }
    }
);

if let TimeoutResult::Ok(value) = result {
    println!("Received: {}", value);
}
```

## リトライ機能

失敗した操作を自動的にリトライします。

### RetryConfig

```rust
use crate::serial::{RetryConfig, retry_with_timeout, RetryResult};

// デフォルト設定（3回リトライ）
let config = RetryConfig::default_retry();

// 高速リトライ（5回、短い遅延）
let config = RetryConfig::quick_retry();

// 持続的リトライ（10回、長いタイムアウト）
let config = RetryConfig::persistent_retry();

// カスタム設定
let config = RetryConfig {
    max_retries: 5,
    timeout: TimeoutConfig::default_timeout(),
    retry_delay: 2000,  // リトライ間の遅延
};
```

### retry_with_timeout

```rust
let result = retry_with_timeout(
    RetryConfig::default_retry(),
    || {
        // 操作を試行
        // 成功時はSome(value)を返す
        if serial_port_ready() {
            Some(read_byte())
        } else {
            None
        }
    }
);

match result {
    RetryResult::Ok(value) => {
        println!("Success: {}", value);
    }
    RetryResult::Failed { attempts, last_error } => {
        println!("Failed after {} attempts", attempts);
    }
}
```

## アダプティブタイムアウト

成功率に基づいて自動的にタイムアウトを調整します。

```rust
use crate::serial::{AdaptiveTimeout, TimeoutConfig, poll_with_timeout};

let mut adaptive = AdaptiveTimeout::new(TimeoutConfig::default_timeout());

// 操作ループ
loop {
    // 現在の設定を取得
    let config = adaptive.current_config();

    // タイムアウト付きで操作を実行
    let result = poll_with_timeout(config, || hardware_ready());

    // 結果を記録して次回の設定を調整
    match result {
        TimeoutResult::Ok(_) => {
            adaptive.record_success();
            // 操作を実行
        }
        TimeoutResult::Timeout { .. } => {
            adaptive.record_failure();
            // エラー処理
        }
    }
}

// 統計情報を取得
let (successes, failures, multiplier) = adaptive.stats();
println!("Success rate: {}%", successes * 100 / (successes + failures));
println!("Current timeout multiplier: {}%", multiplier);

// 統計をリセット
adaptive.reset();
```

## シリアルポートでの統合

シリアルポートモジュールでは、これらの機能が自動的に統合されています。

### 書き込み操作

`poll_and_write`メソッドはアダプティブタイムアウトを使用します：

```rust
// 内部的に自動でアダプティブタイムアウトが適用されます
serial::write_str("Hello, World!\n");
```

### タイムアウト統計の取得

```rust
use crate::serial;

// ポート固有の統計
let (successes, failures, multiplier) = serial::get_timeout_stats();

// グローバル統計
let (timeouts, successful_polls) = serial::get_global_timeout_stats();

// 統計をリセット
serial::reset_timeout_stats();
```

### ハードウェア検出でのリトライ

初期化処理では自動的にリトライロジックが使用されます：

```rust
// 内部でretry_with_timeoutを使用
match serial::init() {
    Ok(()) => println!("Serial port initialized"),
    Err(e) => println!("Failed to initialize: {:?}", e),
}
```

## バックオフ戦略

タイムアウト機能は3種類のバックオフ戦略をサポートします：

### 1. バックオフなし

```rust
BackoffStrategy::None
```

ビジーウェイトなしで連続的にポーリングします。最速ですが、CPUを多く使用します。

### 2. リニアバックオフ

```rust
BackoffStrategy::Linear
```

反復回数に比例して待機時間が増加します（1, 2, 3, ...回のスピンループ）。

### 3. 指数バックオフ

```rust
BackoffStrategy::Exponential { base: 2, max: 100 }
```

指数的に待機時間が増加します（2^1, 2^2, 2^3, ...回のスピンループ、最大値でキャップ）。

## ベストプラクティス

1. **適切な設定の選択**
   - 高速な操作には`short_timeout()`を使用
   - 通常の操作には`default_timeout()`を使用
   - 遅いハードウェアには`long_timeout()`を使用

2. **リトライの使用**
   - 一時的な失敗が予想される操作にはリトライを使用
   - クリティカルな操作には`persistent_retry()`を使用

3. **アダプティブタイムアウト**
   - 繰り返し実行される操作にはアダプティブタイムアウトを使用
   - 定期的に統計をチェックして、パフォーマンスを監視

4. **統計の活用**
   - 定期的に統計情報を確認
   - 高い失敗率が見られる場合は、ハードウェアの問題を調査

## トラブルシューティング

### タイムアウトが頻繁に発生する

```rust
// タイムアウトを増やす
let config = TimeoutConfig {
    max_iterations: 20000,  // デフォルトの2倍
    backoff: BackoffStrategy::Linear,
};
```

### パフォーマンスが低い

```rust
// バックオフを減らす
let config = TimeoutConfig {
    max_iterations: 1000,
    backoff: BackoffStrategy::None,  // バックオフなし
};
```

### ハードウェアが不安定

```rust
// リトライを増やす
let config = RetryConfig {
    max_retries: 10,
    timeout: TimeoutConfig::long_timeout(),
    retry_delay: 5000,
};
```

## まとめ

タイムアウト機能は、以下の利点を提供します：

- **堅牢性**: ハードウェアの失敗やタイムアウトに対して耐性がある
- **適応性**: ハードウェアの特性に応じて自動的に最適化
- **観測可能性**: 詳細な統計情報で問題を診断可能
- **柔軟性**: さまざまなユースケースに対応できる設定オプション

これらの機能により、シリアルポート通信はより信頼性が高く、効率的になります。
