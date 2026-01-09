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

---

### Phase 4: Linux 実起動テスト - 2026-01-09

**目的 (Objective)**:
- 実際の Linux カーネル（最小構成）を起動する
- UART から earlycon 出力を確認する
- Kernel panic (VFS mount 失敗) まで到達する

**制約 (Guardrails)**:
- macOS Apple Silicon 環境
- Hypervisor.framework 使用
- シングル vCPU（マルチ vCPU は次フェーズ）

**タスク (TODOs)**:

#### Week 1: システムレジスタトラップ
- [x] VM Exit で EC=0x18 (MSR/MRS) をハンドリング
- [x] ISS から Op0/Op1/CRn/CRm/Op2/Rt をデコード
- [x] Timer レジスタ (CNTP_*, CNTV_*, CNTFRQ_EL0, CNTPCT_EL0) を対応
- [x] テスト作成

#### Week 2: 割り込み注入機構
- [x] applevisor の割り込み注入 API 調査
- [x] InterruptController と VM 実行ループを統合
- [x] poll_timer_irqs → vCPU にインジェクト
- [x] テスト作成

#### Week 3: WFI/WFE と PSCI
- [ ] EC=0x01 (WFI/WFE) ハンドリング
- [ ] WFI 時に次のタイマーイベントまでスリープ
- [ ] EC=0x16 (HVC) ハンドリング
- [ ] PSCI 最小実装 (VERSION, CPU_OFF, SYSTEM_RESET)

#### Week 4: UART 完全実装
- [ ] PL011 追加レジスタ (IBRD, FBRD, LCR_H, CR, IMSC, RIS, MIS, ICR)
- [ ] MMIO 命令デコード改善
- [ ] テスト作成

#### Week 5: カーネルビルドと Earlycon 起動
- [ ] Linux カーネル v6.x ソース取得
- [ ] 最小構成 defconfig 作成
- [ ] クロスコンパイル環境構築
- [ ] Image (非圧縮カーネル) 生成
- [ ] earlycon_test.rs 作成

#### Week 6: フルブートとデバッグ
- [ ] カーネル起動ログ解析
- [ ] 不足機能の特定と実装
- [ ] デバッグログ強化

**検証手順 (Validation)**:
- Week 1: `mrs x0, cntpct_el0` でカウンタ値が取得できる
- Week 2: タイマー割り込みがゲストに配信される
- Week 3: WFI → タイマー復帰が動作する
- Week 4: earlycon 出力が正しく表示される
- Week 5: "Booting Linux" が UART に表示される
- Week 6: "Kernel panic - VFS" まで到達

**メモリマップ**:
```
0x0800_0000 - GIC Distributor
0x0801_0000 - GIC CPU Interface
0x0900_0000 - UART (PL011)
0x0A00_0000 - VirtIO Block
0x4000_0000 - RAM ベース
0x4008_0000 - Linux カーネル
0x4400_0000 - Device Tree (DTB)
```

**リスクと緩和策**:
| リスク | 緩和策 |
|--------|--------|
| applevisor 割り込み注入 API 不十分 | VGIC レジスタ直接操作 |
| 未知のシステムレジスタアクセス | 警告ログ + 0 返却 |
| MMU 有効化で問題 | 最初は MMU なしでテスト |

**進捗ログ (Progress Log)**:
- [2026-01-09] Phase 4 計画策定
- [2026-01-09] Week 1 完了: システムレジスタトラップ
  - EC=0x18 (MSR/MRS) ハンドリング実装
  - ISS フィールドのデコード (Op0/Op1/CRn/CRm/Op2/Rt)
  - TimerReg::from_encoding() でエンコーディングからレジスタを識別
  - tests/sysreg_test.rs に 7 件の統合テスト追加
  - 発見: Apple Silicon は物理タイマーレジスタ (CNTP_*) をトラップしない場合がある
- [2026-01-09] Week 2 完了: 割り込み注入機構
  - applevisor API: vcpu.set_pending_interrupt(InterruptType::IRQ, bool) を確認
  - InterruptController を Hypervisor 構造体に統合
  - VM 実行ループで poll_timer_irqs() → set_pending_interrupt() を実行
  - tests/interrupt_injection_test.rs に 6 件の統合テスト追加
  - 全 108 テスト通過
