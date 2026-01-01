# Development Plan

## 実行計画 (Exec Plans)

### macOS Hypervisor.framework を使ったハイパーバイザー構築 - 2026-01-01

**目的 (Objective)**:
- macOS の Hypervisor.framework を使用して、Rust でシンプルなハイパーバイザーを構築する
- 最終的に簡単なゲストコードを実行できるようにする

**参考資料**:
- 元記事: https://iovec.net/2024-01-29 (KVM ベース、Linux 向け)
- macOS 版として Hypervisor.framework に移植

**制約 (Guardrails)**:
- macOS 環境で動作すること
- Rust を使用
- Hypervisor.framework の API を直接利用

**タスク (TODOs)**:
- [ ] Hypervisor.framework の API を調査
- [ ] Rust プロジェクトの初期セットアップ
- [ ] Hypervisor.framework の Rust バインディングを選定/作成
- [ ] 基本的な VM 作成・vCPU 作成の実装
- [ ] ゲストメモリのマッピング
- [ ] シンプルなゲストコード（Hello World 相当）の実行
- [ ] VM Exit ハンドリングの実装

**検証手順 (Validation)**:
- `cargo build` が成功すること
- `cargo run` でゲストコードが実行されること
- 期待する出力が得られること

**未解決の質問 (Open Questions)**:
- Hypervisor.framework の Rust バインディングは既存のものがあるか？
- Apple Silicon (ARM64) と Intel (x86_64) どちらをターゲットにするか？

**進捗ログ (Progress Log)**:
- [2026-01-01 開始] プロジェクト開始、調査フェーズ
- [2026-01-01] Rust プロジェクト初期化、applevisor クレート選定
- [2026-01-01] 基本的なハイパーバイザー実装完了
  - VirtualMachine, Vcpu, Mapping の作成
  - ゲストコード (mov x0, #42; brk #0) の書き込み
  - vCPU 実行、BRK 命令での VM Exit 確認
  - x0 = 42 が正しく設定されていることを確認

**振り返り (Retrospective)**:

##### 問題 (Problem)
applevisor クレートの API がドキュメントと実際の実装で異なっていた。
- `Mapping::new()` には `Mappable` トレイトの import が必要
- `VcpuExit` ではなく `get_exit_info()` メソッドを使用
- `ExitReason::Exception` ではなく `ExitReason::EXCEPTION` (大文字)

##### 根本原因 (Root Cause)
docs.rs のドキュメントが簡略化されており、実際の API 詳細が不十分だった。

##### 予防策 (Prevention)
- クレートの GitHub リポジトリで実際のソースコードを確認する
- コンパイルエラーのヒントを活用して API を修正する
