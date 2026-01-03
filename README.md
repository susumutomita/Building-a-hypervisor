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

# Hypervisor 権限で署名
codesign --sign - --entitlements entitlements.xml --deep --force target/debug/hypervisor

# 実行
./target/debug/hypervisor
```

## 実行例

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

## 実装の流れ

| ステップ | 内容 |
|---------|------|
| 1 | VirtualMachine を作成 |
| 2 | vCPU を作成 |
| 3 | ゲストメモリをマッピング (4KB @ 0x10000) |
| 4 | ARM64 ゲストコードを書き込み |
| 5 | vCPU レジスタを設定 (PC, CPSR) |
| 6 | vCPU を実行し、BRK 命令で VM Exit をキャッチ |

## 参考資料

- [Building a hypervisor - Part 1: Hello, World!](https://iovec.net/2024-01-29) - 元記事 (KVM ベース)
- [applevisor](https://github.com/impalabs/applevisor) - Apple Silicon 向け Hypervisor.framework バインディング
- [Apple Hypervisor Documentation](https://developer.apple.com/documentation/hypervisor)

## ライセンス

MIT
