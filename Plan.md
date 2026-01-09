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
- [x] EC=0x01 (WFI/WFE) ハンドリング
- [x] WFI 時に次のタイマーイベントまでスリープ
- [x] EC=0x16 (HVC) ハンドリング
- [x] PSCI 最小実装 (VERSION, CPU_OFF, SYSTEM_RESET)

#### Week 4: UART 完全実装
- [x] PL011 追加レジスタ (IBRD, FBRD, LCR_H, CR, IMSC, RIS, MIS, ICR)
- [x] MMIO 命令デコード改善
- [x] テスト作成

#### Week 5: カーネルビルドと Earlycon 起動
- [x] Linux カーネル v6.x ソース取得
- [x] 最小構成 defconfig 作成
- [x] クロスコンパイル環境構築
- [x] Image (非圧縮カーネル) 生成
- [x] earlycon_test.rs 作成

#### Week 6: フルブートとデバッグ
- [x] ARM64 命令エンコーディングバグ修正
- [x] HVC PC 進行バグ修正
- [x] Hypervisor.framework テスト全件通過確認
- [x] 実 Linux カーネルでの起動テスト ✅ v6.6 起動成功！
- [x] カーネル起動ログ解析
- [ ] GIC MMIO ハンドラー登録

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
  - PR #33 作成、CI 通過
  - CI 環境では Hypervisor.framework が利用できないため、統合テストに #[ignore] 追加
- [2026-01-09] Week 3 完了: WFI/WFE と PSCI
  - EC=0x01 (WFI/WFE) ハンドリング実装
    - WFI 時にタイマー IRQ をポーリング
    - ペンディング IRQ があれば即座に続行
    - なければ次のタイマーイベントまでスリープ（最大 10ms）
  - EC=0x16 (HVC) ハンドリングと PSCI 実装
    - PSCI_VERSION (0x84000000): バージョン 1.0 を返す
    - PSCI_CPU_SUSPEND (0xC4000001): 短いスリープ後に続行
    - PSCI_CPU_OFF (0x84000002): VM Exit
    - PSCI_CPU_ON (0xC4000003): ALREADY_ON を返す（シングル vCPU）
    - PSCI_AFFINITY_INFO (0xC4000004): ON を返す
    - PSCI_SYSTEM_OFF (0x84000008): VM Exit
    - PSCI_SYSTEM_RESET (0x84000009): VM Exit
    - PSCI_FEATURES (0x8400000A): 対応関数をクエリ
  - tests/wfi_psci_test.rs に 6 件の統合テスト追加
  - 全 95 テスト通過（89 unit + 3 integration + 3 doc）
- [2026-01-09] Week 4 完了: UART 完全実装
  - PL011 全レジスタ実装
    - DR, RSR_ECR, FR, IBRD, FBRD, LCR_H, CR, IFLS, IMSC, RIS, MIS, ICR, DMACR
    - Peripheral ID / Cell ID レジスタ（PL011 識別用）
    - Flag Register: TXFE, RXFE, CTS, DSR, DCD
    - Control Register: UARTEN, TXE, RXE 等
    - Interrupt 管理: IMSC, RIS, MIS, ICR
  - Data Abort ISS デコード改善
    - ISV (Instruction Syndrome Valid) チェック
    - SRT (Syndrome Register Transfer) から転送レジスタ取得
    - FnV (FAR not Valid) チェック
  - UART テスト 8 件追加（計 97 unit tests）
- [2026-01-09] Week 5 完了: カーネルビルドと Earlycon 起動
  - earlycon_test.rs 作成
    - UART への単一文字出力テスト
    - Flag Register 読み取りテスト
    - Control Register 読み書きテスト
    - earlycon シーケンステスト
  - mini_kernel_test.rs 作成
    - UART に "Hello" を出力するミニカーネル
    - PSCI_SYSTEM_OFF で終了
    - Device Tree 生成テスト
    - KernelImage 作成テスト
  - docs/linux-kernel-build.md 作成
    - クロスコンパイル環境構築手順
    - カーネル設定（defconfig）
    - Docker を使用したビルド手順
    - トラブルシューティング
  - テスト: 105 passed（97 unit + 8 integration）
- [2026-01-09] Week 6 進行中: フルブートとデバッグ
  - ARM64 命令エンコーディング修正
    - MOVK 命令のエンコーディングが間違っていた
    - 例: MOVK X0, #0x8400, LSL #16 の正しいエンコーディングは 0xF2B0_8000
    - 問題: hw フィールド (bits 22:21) が 01 (LSL #16) ではなく 00 (no shift) になっていた
    - 修正: earlycon_test.rs, mini_kernel_test.rs, wfi_psci_test.rs
  - HVC PC 進行バグ修正（重要な発見）
    - 問題: HVC ハンドラで PC を +4 していたが、Hypervisor.framework は HVC 時に PC を既に進めていた
    - 原因: HVC は "preferred return" exception なので、ELR_EL2 = HVC + 4 がセットされる
    - 修正: handle_hvc() で PC を進めないように変更
    - 影響を受けたテスト: wfi_psci_test.rs の 4 件の HVC テストが全て失敗していた
  - テスト結果確認
    - mini_kernel テスト成功: UART に "Hello" を出力し、EC=0x16 (HVC) で終了
    - 全 Hypervisor.framework テスト成功: 24 件（earlycon 3, interrupt 6, mini_kernel 1, sysreg 7, wfi_psci 6）
    - 全ユニットテスト成功: 97 件
  - 発見: ARM64 例外と PC 進行の動作
    - Data Abort (EC=0x24): ELR = faulting PC → 手動で +4 必要
    - MSR/MRS trap (EC=0x18): ELR = instruction PC → 手動で +4 必要
    - WFI/WFE (EC=0x01): ELR = WFI/WFE PC → 手動で +4 必要
    - HVC (EC=0x16): ELR = preferred return (HVC + 4) → +4 不要
- [2026-01-09] 🎉 Linux カーネル v6.6 起動成功！
  - Docker で ARM64 Linux カーネルをクロスコンパイル
    - Dockerfile.linux-build 作成
    - scripts/build-linux-kernel.sh 作成
    - 42MB の Image 生成
  - Data Abort ハンドラーの IPA 取得バグ修正
    - 問題: FAR_EL1 から 0 が返され、フォールバックで X1 の値（書き込み文字）をアドレスとして使用していた
    - 原因: Stage 2 フォールトでは FAR_EL1 ではなく exit_info.exception.physical_address を使用すべき
    - 修正: handle_data_abort() に fault_ipa 引数を追加し、physical_address を使用
  - 未知のシステムレジスタアクセスをエミュレート
    - 読み取り時は 0 を返し、書き込み時は無視して続行
  - linux_boot_test.rs 作成
    - UartCollector で UART 出力をキャプチャ
    - 3904 バイトの正常な起動ログを確認
  - 確認できた起動ログ
    ```
    Booting Linux on physical CPU 0x0000000000 [0x610f0000]
    Linux version 6.6.0 ...
    Machine model: hypervisor-virt
    earlycon: pl11 at MMIO 0x0000000009000000
    printk: bootconsole [pl11] enabled
    arch_timer: cp15 timer(s) running at 24.00MHz (virt)
    ...
    Calibrating delay loop ... 48.00 BogoMIPS
    LSM: initializing lsm=capability,integrity
    Mount-cache hash table entries: 512
    ```
  - 残課題: GIC レジスタへの MMIO アクセス (0x8000xxx) が未処理
