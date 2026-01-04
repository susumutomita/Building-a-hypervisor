# Building a Hypervisor

macOS Hypervisor.framework を使用した Apple Silicon 向けハイパーバイザーの実装。

## 概要

[KVM ベースのハイパーバイザー構築記事](https://iovec.net/2024-01-29)を参考に、macOS の Hypervisor.framework を使って Rust でハイパーバイザーを実装するプロジェクト。

## 必要環境

- macOS (Apple Silicon)
- Rust toolchain
- Xcode Command Line Tools

## ビルド方法

```bash
# ビルド
cargo build

# 実行 (簡単なデモ)
cargo run

# より実用的な例を実行
cargo run --example fibonacci
```

**注意**: macOS では Hypervisor.framework を使用するため、実行バイナリに署名が必要な場合があります。署名は自動的に行われますが、エラーが発生した場合は以下を実行してください。

```bash
codesign --entitlements /tmp/entitlements.plist -s - target/debug/hypervisor
```

## 実行例

### 基本デモ (`cargo run`)

```
=== macOS Hypervisor Demo (Apple Silicon) ===

[1] VirtualMachine を作成中...
    ✓ VM 作成完了
[2] vCPU を作成中...
    ✓ vCPU 作成完了
[3] ゲストメモリをマッピング中...
    ✓ メモリマッピング完了 (ゲストアドレス: 0x10000, サイズ: 4096 bytes)
[4] ゲストコードを書き込み中...
    ✓ ゲストコード書き込み完了 (8 bytes)
[5] vCPU レジスタを設定中...
    ✓ PC = 0x10000
    ✓ CPSR = 0x3c4 (EL1h)
    ✓ デバッグ例外トラップ有効化

[6] vCPU を実行中...
---
VM Exit:
  - Reason: EXCEPTION
  - PC: 0x10004
  - X0: 42 (0x2a)

✓ BRK 命令を検出!
  ゲストが x0 = 42 を設定して BRK を呼び出しました。

=== ハイパーバイザーデモ完了 ===
```

### Fibonacci 数列計算 (`cargo run --example fibonacci`)

```
=== Fibonacci 数列計算デモ ===

[1] ハイパーバイザーを初期化中...
    ✓ ゲストアドレス: 0x10000
[2] ゲストコードを書き込み中...
    ✓ 9 命令を書き込み完了
[3] ゲストプログラムを実行中...

---
VM Exit:
  - Reason: EXCEPTION
  - PC: 0x10020

レジスタ:
  - X0: 55 (F(10))
  - X1: 89 (F(11))
  - X2: 0 (ループカウンタ)

✓ 計算結果: F(10) = 55
  (期待値: 55)

✅ 正しい結果です！

=== デモ完了 ===
```

## 実装の流れ

### 共通ライブラリ (`src/lib.rs`)

ゲストプログラムを簡単に作成できる `Hypervisor` 構造体を提供します。

```rust
pub struct Hypervisor {
    vm: VirtualMachine,
    vcpu: Vcpu,
    mapping: Mapping,
    guest_addr: u64,
}
```

主な機能：

- `new(guest_addr, mem_size)`: VM とメモリを初期化
- `write_instructions(&[u32])`: ARM64 命令列を書き込み
- `write_data(offset, value)`: ゲストメモリにデータを書き込み
- `read_data(offset)`: ゲストメモリからデータを読み込み
- `run(max_iterations, step_callback)`: vCPU を実行

### 基本的な実装ステップ

| ステップ | 内容 |
|---------|------|
| 1 | `Hypervisor::new()` で VM、vCPU、メモリを初期化 |
| 2 | `write_instructions()` で ARM64 ゲストコードを書き込み |
| 3 | 必要に応じて `write_data()` でデータを書き込み |
| 4 | `run()` で vCPU を実行し、BRK 命令で VM Exit をキャッチ |
| 5 | 戻り値の `VmExitInfo` からレジスタ状態を取得 |

## Examples

### `fibonacci.rs`

VM 内で Fibonacci 数列を計算するプログラム。

**実装内容**：
- ARM64 アセンブリで反復的に F(10) を計算
- レジスタのみを使用（メモリアクセスなし）
- BRK 命令で計算完了を通知

**学べること**：
- 基本的なゲストプログラムの作成方法
- ARM64 命令のエンコーディング
- レジスタ操作とループ制御
- VM Exit のハンドリング

**実行方法**：
```bash
cargo run --example fibonacci
```

## 参考資料

- [Building a hypervisor - Part 1: Hello, World!](https://iovec.net/2024-01-29) - 元記事 (KVM ベース)
- [applevisor](https://github.com/impalabs/applevisor) - Apple Silicon 向け Hypervisor.framework バインディング
- [Apple Hypervisor Documentation](https://developer.apple.com/documentation/hypervisor)

## ライセンス

MIT
