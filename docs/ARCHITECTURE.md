# ハイパーバイザーの仕組み - ステップバイステップ解説

このドキュメントでは、macOS Hypervisor.framework を使用したハイパーバイザーの動作原理とコードの詳細を解説する。

## 目次

1. [ハイパーバイザーとは](#ハイパーバイザーとは)
2. [ARM64 仮想化拡張機能](#arm64-仮想化拡張機能)
3. [全体アーキテクチャ](#全体アーキテクチャ)
4. [コード解説](#コード解説)
5. [ARM64 命令の詳細](#arm64-命令の詳細)
6. [VM Exit の仕組み](#vm-exit-の仕組み)
7. [メモリ仮想化の詳細](#メモリ仮想化の詳細)

---

## ハイパーバイザーとは

ハイパーバイザーは、物理ハードウェア上で複数の仮想マシン (VM) を実行するためのソフトウェア層である。

```
┌─────────────────────────────────────────────────┐
│                  ゲスト OS                       │
│  (仮想マシン内で動作するコード)                    │
├─────────────────────────────────────────────────┤
│               ハイパーバイザー                    │
│  (VM の作成、メモリ管理、CPU 仮想化)              │
├─────────────────────────────────────────────────┤
│           Hypervisor.framework                  │
│  (macOS が提供するハイパーバイザー API)           │
├─────────────────────────────────────────────────┤
│              ハードウェア (CPU)                  │
│  (ARM64 の仮想化支援機能)                        │
└─────────────────────────────────────────────────┘
```

### なぜハードウェア仮想化が必要か

従来のエミュレーション（QEMU など）は、すべての命令をソフトウェアで解釈・実行するため遅い。ハードウェア仮想化では、CPU が直接ゲストコードを実行し、特権命令や I/O アクセス時のみハイパーバイザーに制御を戻す（VM Exit）。これにより、ほぼネイティブに近い速度で仮想マシンを実行できる。

---

## ARM64 仮想化拡張機能

ARM64 には、ハードウェアレベルで仮想化をサポートする機能が組み込まれている。

### Exception Level (EL) アーキテクチャ

ARM64 は、4 つの特権レベル (Exception Level) を持つ階層構造になっている。

```
┌─────────────────────────────────────────────────────┐
│  EL3: Secure Monitor (ARM Trusted Firmware)         │
│  - セキュアワールドと通常ワールドの切り替え             │
│  - システムの最高権限                                 │
└─────────────────────────────────────────────────────┘
                          ▲
                          │ SMC (Secure Monitor Call)
                          ▼
┌─────────────────────────────────────────────────────┐
│  EL2: Hypervisor (macOS Kernel + Hypervisor.framework)│
│  - 仮想化制御                                         │
│  - Stage-2 address translation                      │
│  - VM の作成・削除・実行                              │
└─────────────────────────────────────────────────────┘
                          ▲
                          │ HVC (Hypervisor Call)
                          │ または Trap (VM Exit)
                          ▼
┌─────────────────────────────────────────────────────┐
│  EL1: OS Kernel (ゲスト OS / このハイパーバイザーのゲストコード)│
│  - メモリ管理 (Stage-1 translation)                   │
│  - プロセス管理                                       │
│  - デバイスドライバ                                   │
└─────────────────────────────────────────────────────┘
                          ▲
                          │ SVC (Supervisor Call)
                          ▼
┌─────────────────────────────────────────────────────┐
│  EL0: User Application                              │
│  - 一般的なアプリケーションが動作                       │
│  - 最も制限された権限レベル                            │
└─────────────────────────────────────────────────────┘
```

### 各 EL の特権

| Exception Level | 権限 | アクセス可能なリソース |
|----------------|------|---------------------|
| **EL3** | 最高権限 | すべてのシステムレジスタ、セキュアメモリ |
| **EL2** | ハイパーバイザー権限 | EL1/EL0 の制御、仮想化レジスタ (HCR_EL2, VTTBR_EL2 など) |
| **EL1** | カーネル権限 | MMU、例外処理、システムレジスタ (一部) |
| **EL0** | ユーザー権限 | 汎用レジスタ、通常メモリ |

### EL2 (Hypervisor Mode) の役割

EL2 は、仮想化を実現するための特別な特権レベルである。EL2 で動作するハイパーバイザーは、以下の制御を行う。

#### 1. HCR_EL2 (Hypervisor Configuration Register)

EL1 以下の動作を制御するレジスタ。

```
HCR_EL2 の主要ビット:

[0]   VM    - Virtualization MMU enable (Stage-2 translation を有効化)
[1]   SWIO  - Set/Way Invalidation Override
[3]   PTW   - Protected Table Walk
[4]   AMO   - Asynchronous abort mask override
[5]   IMO   - IRQ mask override (物理 IRQ を仮想 IRQ にルーティング)
[6]   FMO   - FIQ mask override (物理 FIQ を仮想 FIQ にルーティング)
[27]  TGE   - Trap General Exceptions (EL0 の例外を EL2 にトラップ)
[31]  RW    - Execution state (1 = EL1 は AArch64, 0 = AArch32)
```

**VM ビット (ビット 0) の役割:**

このビットを 1 に設定すると、EL1/EL0 のメモリアクセスに Stage-2 address translation が適用される。これにより、ゲスト物理アドレス (IPA: Intermediate Physical Address) をホスト物理アドレス (PA: Physical Address) に変換できる。

#### 2. VTTBR_EL2 (Virtualization Translation Table Base Register)

Stage-2 translation のページテーブルベースアドレスを保持するレジスタ。

```
VTTBR_EL2:

[63:48]  VMID   - Virtual Machine Identifier (複数 VM を区別)
[47:1]   BADDR  - Translation table base address
[0]      CnP    - Common not Private (TLB 最適化)
```

**VMID の役割:**

複数の仮想マシンが同時に動作する場合、各 VM に異なる VMID を割り当てる。TLB (Translation Lookaside Buffer) エントリに VMID が記録されるため、VM 切り替え時に TLB をフラッシュする必要がない（パフォーマンス向上）。

#### 3. VBAR_EL2 (Vector Base Address Register)

EL2 の例外ベクターテーブルのアドレスを保持する。

```
例外ベクターテーブルの構造:

VBAR_EL2 + 0x000: Synchronous exception (from current EL with SP_EL0)
VBAR_EL2 + 0x080: IRQ/vIRQ (from current EL with SP_EL0)
VBAR_EL2 + 0x100: FIQ/vFIQ (from current EL with SP_EL0)
VBAR_EL2 + 0x180: SError/vSError (from current EL with SP_EL0)
VBAR_EL2 + 0x200: Synchronous exception (from current EL with SP_ELx)
VBAR_EL2 + 0x280: IRQ/vIRQ (from current EL with SP_ELx)
...
VBAR_EL2 + 0x400: Synchronous exception (from lower EL using AArch64)
VBAR_EL2 + 0x480: IRQ/vIRQ (from lower EL using AArch64)
...
```

### macOS での EL2 と Hypervisor.framework

macOS では、カーネル自体が EL2 で動作し、Hypervisor.framework がカーネル内部で仮想化機能を提供する。

```
┌─────────────────────────────────────────┐
│  macOS Kernel (XNU) @ EL2               │
│  ┌─────────────────────────────────┐    │
│  │  Hypervisor.framework           │    │
│  │  - hv_vm_create()               │    │
│  │  - hv_vcpu_create()             │    │
│  │  - hv_vm_map()                  │    │
│  │  - HCR_EL2, VTTBR_EL2 を設定    │    │
│  └─────────────────────────────────┘    │
└─────────────────────────────────────────┘
                 ▲
                 │ システムコール (ioctl など)
                 ▼
┌─────────────────────────────────────────┐
│  ユーザープロセス @ EL0                  │
│  (このハイパーバイザープログラム)         │
└─────────────────────────────────────────┘
```

**重要な制約:**

- ユーザープロセスは直接 EL2 のレジスタにアクセスできない
- Hypervisor.framework の API を通じて間接的に仮想化機能を利用する
- ゲストコードは EL1 で実行され、EL2 (macOS カーネル) に監視される

### VHE (Virtualization Host Extensions)

ARMv8.1 以降では、VHE という拡張機能が導入されている。VHE を有効にすると、ホスト OS を EL2 で直接実行できる（従来は EL1 で動作）。

**VHE の利点:**

1. ホスト OS が EL2 権限を持つため、ゲスト VM との切り替えコストが削減される
2. ホスト OS のページテーブルをそのまま使用でき、Stage-1/Stage-2 の分離が不要
3. TLB の効率が向上

macOS (Apple Silicon) は VHE を活用して、カーネルを EL2 で実行している。

---

## 全体アーキテクチャ

### 実行フロー

```
┌──────────────────┐
│  1. VM 作成       │  VirtualMachine::new()
└────────┬─────────┘
         ▼
┌──────────────────┐
│  2. vCPU 作成     │  Vcpu::new()
└────────┬─────────┘
         ▼
┌──────────────────┐
│  3. メモリ確保    │  Mapping::new() + map()
└────────┬─────────┘
         ▼
┌──────────────────┐
│  4. コード書込み   │  write_dword()
└────────┬─────────┘
         ▼
┌──────────────────┐
│  5. レジスタ設定  │  set_reg(PC, CPSR)
└────────┬─────────┘
         ▼
┌──────────────────┐
│  6. vCPU 実行     │  vcpu.run()
└────────┬─────────┘
         ▼
    ┌────┴────┐
    ▼         ▼
 ゲスト     VM Exit
 コード    (例外発生)
 実行         │
    │         ▼
    │    ┌──────────────┐
    │    │ Exit 処理     │
    │    │ (理由を判定)   │
    │    └──────┬───────┘
    │           │
    └───────────┘
       (ループ)
```

### 主要コンポーネント

| コンポーネント | 役割 |
|--------------|------|
| `VirtualMachine` | VM 全体を管理。プロセスごとに 1 つのみ作成可能 |
| `Vcpu` | 仮想 CPU。レジスタ状態を保持し、ゲストコードを実行 |
| `Mapping` | ゲストメモリ領域。ホストメモリをゲストアドレス空間にマップ |

---

## コード解説

### Step 1: VirtualMachine の作成

```rust
let _vm = VirtualMachine::new()?;
```

**何が起きているか:**

1. Hypervisor.framework の `hv_vm_create()` を呼び出す
2. カーネルが仮想化支援機能を初期化
3. プロセスに VM コンテキストが割り当てられる

**制約:**
- 1 プロセスにつき 1 VM のみ
- `com.apple.security.hypervisor` エンタイトルメントが必要

**なぜ `_vm` とアンダースコアをつけているか:**
- 変数を直接使用しないが、スコープ内で VM を維持する必要がある
- `_vm` がドロップされると VM が破棄される

---

### Step 2: vCPU の作成

```rust
let vcpu = Vcpu::new()?;
```

**何が起きているか:**

1. `hv_vcpu_create()` を呼び出す
2. 仮想 CPU のコンテキスト（レジスタ、状態）が作成される
3. vCPU は独自のスレッドで実行可能（今回は単一スレッド）

**vCPU の状態:**

```
┌─────────────────────────────────┐
│           vCPU                  │
├─────────────────────────────────┤
│  汎用レジスタ: X0-X30           │
│  PC (プログラムカウンタ)         │
│  SP (スタックポインタ)           │
│  CPSR (ステータスレジスタ)       │
│  システムレジスタ群              │
└─────────────────────────────────┘
```

---

### Step 3: ゲストメモリのマッピング

```rust
let guest_addr: u64 = 0x10000;
let mem_size: usize = 0x1000; // 4KB

let mut mem = Mapping::new(mem_size)?;
mem.map(guest_addr, MemPerms::RWX)?;
```

**何が起きているか:**

1. `Mapping::new(0x1000)`: ホスト側で 4KB のメモリを確保
2. `mem.map(0x10000, RWX)`: そのメモリをゲストアドレス `0x10000` にマップ

**メモリマッピングの構造:**

```
ホスト側                        ゲスト側
(物理メモリ)                    (仮想アドレス空間)

┌──────────┐                   ┌──────────┐
│          │                   │          │
│  heap    │                   │ 0x00000  │ (未マップ)
│          │                   │          │
├──────────┤ ◄─── マップ ────► ├──────────┤
│  4KB     │                   │ 0x10000  │ ゲストコード
│  確保    │                   │          │ が実行される
├──────────┤                   ├──────────┤
│          │                   │ 0x11000  │ (未マップ)
└──────────┘                   └──────────┘
```

**MemPerms::RWX の意味:**
- **R** (Read): 読み取り可能
- **W** (Write): 書き込み可能
- **X** (Execute): 実行可能

コードを実行するため、実行権限 (X) が必須。

---

### Step 4: ゲストコードの書き込み

```rust
// mov x0, #42 (0xD2800540)
mem.write_dword(guest_addr, 0xD2800540)?;
// brk #0 (0xD4200000)
mem.write_dword(guest_addr + 4, 0xD4200000)?;
```

**何が起きているか:**

ARM64 の機械語命令を直接メモリに書き込んでいる。

**メモリ上の配置:**

```
アドレス      内容              命令
0x10000      D2 80 05 40      mov x0, #42
0x10004      D4 20 00 00      brk #0
```

各命令は 4 バイト（32 ビット）固定長。

---

### Step 5: vCPU レジスタの設定

```rust
vcpu.set_reg(Reg::PC, guest_addr)?;    // PC = 0x10000
vcpu.set_reg(Reg::CPSR, 0x3c4)?;       // CPSR = EL1h モード
vcpu.set_trap_debug_exceptions(true)?;
```

**各レジスタの役割:**

#### PC (Program Counter)
- 次に実行する命令のアドレス
- `0x10000` に設定 → ゲストコードの先頭から実行開始

#### CPSR (Current Program Status Register) / PSTATE

ARM64 では、CPSR は PSTATE (Process State) と呼ばれる。PSTATE は物理的な単一のレジスタではなく、複数のシステムレジスタの論理的な集合である。

**PSTATE の構成要素:**

```
┌─────────────────────────────────────────────────┐
│  PSTATE (Process State)                         │
├─────────────────────────────────────────────────┤
│  NZCV    - 条件フラグ (Negative, Zero, Carry, oVerflow) │
│  DAIF    - 割り込みマスク (Debug, SError, IRQ, FIQ)     │
│  CurrentEL - 現在の Exception Level (EL0-EL3)        │
│  SPSel   - スタックポインタ選択 (SP_EL0 or SP_ELx)     │
│  SS      - ソフトウェアステップビット                  │
│  IL      - Illegal Execution State                   │
│  ...     - その他の状態ビット                         │
└─────────────────────────────────────────────────┘
```

**CPSR = 0x3c4 の詳細解析:**

```
CPSR = 0x3c4 = 0b0000_0011_1100_0100

ビット範囲  フィールド   値     意味
[31:28]     NZCV        0000   条件フラグすべてクリア
                               N=0 (結果は非負)
                               Z=0 (結果は非ゼロ)
                               C=0 (キャリーなし)
                               V=0 (オーバーフローなし)

[27:10]     (reserved)  0...0  予約ビット

[9]         D           1      Debug exceptions masked (デバッグ例外をマスク)
[8]         A           1      SError masked (非同期アボートをマスク)
[7]         I           1      IRQ masked (割り込み要求をマスク)
[6]         F           1      FIQ masked (高速割り込みをマスク)

[5]         (reserved)  0      予約ビット

[4]         M[4]        0      AArch64 実行状態
                               (1 なら AArch32)

[3:0]       M[3:0]      0100   Exception Level とスタックポインタ選択
                               0100 = EL1h (EL1 with SP_EL1)
```

**M[3:0] の値と意味:**

| M[3:0] | モード | Exception Level | スタックポインタ |
|--------|--------|----------------|---------------|
| 0000 | EL0t | EL0 | SP_EL0 |
| 0100 | EL1t | EL1 | SP_EL0 |
| 0101 | EL1h | EL1 | SP_EL1 |
| 1000 | EL2t | EL2 | SP_EL0 |
| 1001 | EL2h | EL2 | SP_EL2 |
| 1100 | EL3t | EL3 | SP_EL0 |
| 1101 | EL3h | EL3 | SP_EL3 |

**"t" と "h" の意味:**

- **"t" (thread mode)**: SP_EL0 を使用（通常、ユーザープロセスのスタック）
- **"h" (handler mode)**: SP_ELx を使用（その EL 専用のスタック）

**EL1h モード (0x05) とは:**

- Exception Level 1 (カーネルモード相当)
- "h" は Handler モード → SP_EL1 を使用
- ゲスト OS カーネルが動作する権限レベル
- システムレジスタの一部にアクセス可能
- Stage-1 translation を制御可能 (TTBR0_EL1, TTBR1_EL1)

**DAIF ビットの役割 (割り込みマスク):**

```
DAIF = 0b1111 (すべてマスク)

D (Debug)    = 1  → デバッグ例外を無効化
A (SError)   = 1  → 非同期アボートを無効化
I (IRQ)      = 1  → 通常の割り込みを無効化
F (FIQ)      = 1  → 高速割り込みを無効化
```

これらのビットを 1 に設定すると、対応する例外が発生しても CPU が応答しない。ゲストの初期状態では、すべてマスクすることで、予期しない割り込みで VM Exit が発生するのを防ぐ。

**NZCV フラグ (条件フラグ):**

```
NZCV = 0b0000 (すべてクリア)

N (Negative)  = 0  → 演算結果は非負
Z (Zero)      = 0  → 演算結果は非ゼロ
C (Carry)     = 0  → キャリー/ボローなし
V (oVerflow)  = 0  → 符号付きオーバーフローなし
```

これらのフラグは、条件分岐命令 (B.EQ, B.NE, B.LT など) で使用される。

**例: CMP 命令と条件分岐**

```assembly
mov  x0, #10
mov  x1, #20
cmp  x0, x1         // x0 - x1 を計算 (結果は破棄)
                    // x0 < x1 なので N=1, Z=0
b.lt label          // N=1 なら分岐 (Less Than)
```

#### set_trap_debug_exceptions(true)

このメソッドは、Hypervisor.framework に対して「デバッグ例外をトラップして VM Exit として扱う」よう指示する。

**内部動作:**

```
set_trap_debug_exceptions(true) を呼び出すと、
カーネルが HCR_EL2.TDE ビットを 1 に設定する。

HCR_EL2.TDE = 1
  → EL1/EL0 でのデバッグ例外 (BRK, BKPT など) が EL2 にトラップされる
  → VM Exit が発生
  → ハイパーバイザーが制御を取得
```

**これがない場合:**

```
HCR_EL2.TDE = 0
  → デバッグ例外は EL1 で処理される
  → ゲストの VBAR_EL1 が指す例外ハンドラが呼ばれる
  → ハイパーバイザーには制御が戻らない
```

今回のハイパーバイザーでは、ゲストに例外ハンドラが存在しないため、`set_trap_debug_exceptions(true)` を設定しないと BRK 命令で VM が停止する。

---

### Step 6: vCPU の実行と VM Exit ループ

```rust
loop {
    vcpu.run()?;
    let exit_info = vcpu.get_exit_info();
    // ...
}
```

**vcpu.run() の動作:**

1. CPU コンテキストをゲストモードに切り替え
2. ゲストコードを実行
3. VM Exit が発生するまで実行を継続
4. Exit 情報を記録して制御を返す

**VM Exit ループの構造:**

```
┌─────────────────────────────────────────────────────┐
│                    ホスト側                          │
│                                                     │
│   ┌─────────────┐                                   │
│   │ vcpu.run()  │ ──────────────────────┐           │
│   └─────────────┘                       │           │
│         ▲                               ▼           │
│         │                    ┌──────────────────┐   │
│         │                    │  ゲストコード実行  │   │
│         │                    │  (mov x0, #42)   │   │
│         │                    │  (brk #0)        │   │
│         │                    └────────┬─────────┘   │
│         │                             │             │
│         │                      VM Exit (例外)       │
│         │                             │             │
│         │                             ▼             │
│   ┌─────┴─────────────────────────────────────┐     │
│   │              Exit 処理                     │     │
│   │  - exit_info を取得                        │     │
│   │  - 例外の種類を判定                        │     │
│   │  - BRK なら終了、それ以外は継続            │     │
│   └───────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────┘
```

---

### Exit 情報の解析

```rust
if let applevisor::ExitReason::EXCEPTION = exit_info.reason {
    let syndrome = exit_info.exception.syndrome;
    let ec = (syndrome >> 26) & 0x3f;

    if ec == 0x3C {
        // BRK 命令を検出
        break;
    }
}
```

**Exception Syndrome Register (ESR) の構造:**

```
syndrome = 0xf2000000

ビット      フィールド      値
[31:26]     EC             0x3C (BRK instruction)
[25]        IL             1 (32-bit instruction)
[24:0]      ISS            0x000000 (BRK #0)
```

**EC (Exception Class) の主な値:**

| EC | 例外の種類 |
|----|----------|
| 0x00 | Unknown |
| 0x15 | SVC (Supervisor Call) |
| 0x16 | HVC (Hypervisor Call) |
| 0x17 | SMC (Secure Monitor Call) |
| 0x20 | Instruction Abort (lower EL) |
| 0x24 | Data Abort (lower EL) |
| 0x3C | BRK instruction (AArch64) |

---

## ARM64 命令の詳細

### mov x0, #42 (0xD2800540)

```
0xD2800540 = 0b1101_0010_1000_0000_0000_0101_0100_0000

ビット      フィールド      値          意味
[31]        sf             1           64-bit (X レジスタ)
[30:29]     opc            10          MOVZ
[28:23]     100101         固定
[22:21]     hw             00          シフトなし
[20:5]      imm16          0x002A      即値 42 (0x2A = 42)
[4:0]       Rd             00000       X0 レジスタ
```

**MOVZ 命令:**
- レジスタをゼロクリアしてから即値を移動
- `mov x0, #42` は `movz x0, #42, lsl #0` と等価

### brk #0 (0xD4200000)

```
0xD4200000 = 0b1101_0100_0010_0000_0000_0000_0000_0000

ビット      フィールド      値          意味
[31:24]     11010100       固定
[23:21]     001            BRK
[20:5]      imm16          0x0000      即値 0
[4:0]       00000          固定
```

**BRK 命令:**
- ソフトウェアブレークポイント
- デバッガやハイパーバイザーへの制御転送に使用
- 即値はデバッガが識別に使用可能

---

## VM Exit の仕組み

### なぜ VM Exit が必要か

ゲストコードは基本的に自由に実行されるが、以下の場合はハイパーバイザーの介入が必要である。

1. **特権命令**: システムレジスタへのアクセス (MSR/MRS 命令)
2. **I/O アクセス**: デバイスへの読み書き (MMIO: Memory-Mapped I/O)
3. **例外**: BRK、HVC、未定義命令、ページフォルトなど
4. **割り込み**: タイマー、外部割り込み (IMO/FMO ビットで制御)

### VM Entry と VM Exit のハードウェアメカニズム

#### VM Entry (ホスト → ゲスト)

`vcpu.run()` を呼び出すと、以下の処理がハードウェアレベルで実行される。

```
1. ホスト側のレジスタ状態を保存
   - X0-X30 (汎用レジスタ)
   - SP_EL2, ELR_EL2 (スタックポインタ、リンクレジスタ)
   - システムレジスタ

2. ゲスト側のレジスタ状態を復元
   - X0-X30
   - PC (Program Counter) → ELR_EL1
   - SP_EL1
   - CPSR → SPSR_EL1
   - システムレジスタ (TTBR0_EL1, SCTLR_EL1, VBAR_EL1 など)

3. HCR_EL2 を設定
   - VM ビット (Stage-2 translation 有効化)
   - IMO/FMO ビット (割り込みルーティング)
   - TVM ビット (仮想メモリ制御のトラップ)

4. VTTBR_EL2 を設定
   - Stage-2 ページテーブルベースアドレス
   - VMID (この VM の識別子)

5. 例外レベルを EL1 に下げる (ERET 命令相当)
   - SPSR_EL2 → CPSR (ゲストの CPSR に復元)
   - ELR_EL2 → PC (ゲストの PC に復元)
   - EL2 → EL1 に遷移

6. ゲストコードの実行開始
```

**実際の CPU 命令シーケンス (疑似コード):**

```assembly
// ホスト側 (EL2)
stp  x0, x1, [sp, #-16]!     // ホストレジスタを保存
...
ldr  x0, [vcpu_state, #X0]   // ゲストレジスタを復元
ldr  x1, [vcpu_state, #X1]
...
msr  ELR_EL1, x_pc           // ゲスト PC を設定
msr  SPSR_EL1, x_cpsr        // ゲスト CPSR を設定
msr  VTTBR_EL2, x_vttbr      // Stage-2 ページテーブル設定
eret                          // EL1 に遷移 (ゲスト実行開始)
```

#### VM Exit (ゲスト → ホスト)

ゲストで例外が発生すると、以下の処理がハードウェアによって自動的に実行される。

```
1. 例外発生 (BRK 命令など)

2. CPU が自動的に実行:
   a. 現在の CPSR を SPSR_EL2 に保存
   b. 現在の PC を ELR_EL2 に保存
   c. 例外情報を ESR_EL2 に記録
   d. 例外が発生したアドレスを FAR_EL2 に記録 (Data Abort の場合)

3. 例外レベルを EL2 に上げる
   - EL1 → EL2 に遷移
   - PSTATE の割り込みマスクビット (DAIF) を設定

4. VBAR_EL2 + オフセットにジャンプ
   - 例外の種類に応じたベクターアドレスに分岐
   - 例: Synchronous exception from lower EL → VBAR_EL2 + 0x400

5. ハイパーバイザーの例外ハンドラが実行
   - ゲストのレジスタ状態を保存
   - ESR_EL2 を読み取って例外の種類を判定
   - 必要に応じて処理 (エミュレーション、ゲストへの例外注入など)

6. Hypervisor.framework が hv_vcpu_run() から復帰
   - ユーザープロセスに制御が戻る
```

**レジスタの変化:**

```
VM Exit 前 (ゲスト @ EL1):
  PC      = 0x10004 (brk 命令のアドレス)
  CPSR    = 0x3c4   (EL1h, 割り込みマスク)
  X0      = 42
  CurrentEL = EL1

↓ BRK 命令実行

VM Exit 後 (ホスト @ EL2):
  ELR_EL2  = 0x10004          ← ゲストの PC が保存された
  SPSR_EL2 = 0x3c4            ← ゲストの CPSR が保存された
  ESR_EL2  = 0xf2000000       ← 例外情報 (EC=0x3C: BRK)
  PC       = VBAR_EL2 + 0x400 ← 例外ハンドラにジャンプ
  CurrentEL = EL2
```

### Exception Syndrome Register (ESR_EL2) の詳細

ESR_EL2 は、VM Exit の原因を詳細に記録するレジスタである。

```
ESR_EL2 の構造:

[63:37]  RES0      (予約)
[36]     ISV       (Instruction Syndrome Valid)
[31:26]  EC        (Exception Class) ← 例外の種類
[25]     IL        (Instruction Length: 0=16-bit, 1=32-bit)
[24:0]   ISS       (Instruction Specific Syndrome) ← 例外固有の情報
```

#### EC (Exception Class) の主要な値

| EC (6 bits) | 例外の種類 | 説明 |
|------------|----------|------|
| 0x00 | Unknown | 不明な例外 |
| 0x01 | WFI/WFE trap | Wait For Interrupt/Event のトラップ |
| 0x03 | MCR/MRC (CP15) | システムレジスタアクセス (32-bit) |
| 0x04 | MCRR/MRRC (CP15) | システムレジスタアクセス (64-bit) |
| 0x05 | MCR/MRC (CP14) | デバッグレジスタアクセス |
| 0x07 | FPID trap | 浮動小数点命令のトラップ |
| 0x0E | Illegal Execution State | 不正な実行状態 |
| 0x11 | SVC (AArch32) | Supervisor Call (32-bit) |
| 0x12 | HVC (AArch32) | Hypervisor Call (32-bit) |
| 0x15 | SVC (AArch64) | Supervisor Call (64-bit) |
| 0x16 | HVC (AArch64) | Hypervisor Call (64-bit) |
| 0x17 | SMC (AArch64) | Secure Monitor Call (64-bit) |
| 0x18 | MSR/MRS (AArch64) | システムレジスタアクセス |
| 0x20 | Instruction Abort (lower EL) | 命令フェッチ失敗 (ページフォルトなど) |
| 0x21 | Instruction Abort (same EL) | 命令フェッチ失敗 (同じ EL) |
| 0x22 | PC alignment fault | PC のアライメント違反 |
| 0x24 | Data Abort (lower EL) | データアクセス失敗 (ページフォルトなど) |
| 0x25 | Data Abort (same EL) | データアクセス失敗 (同じ EL) |
| 0x26 | SP alignment fault | スタックポインタのアライメント違反 |
| 0x30 | Breakpoint (lower EL) | ブレークポイント (BKPT 命令) |
| 0x31 | Breakpoint (same EL) | ブレークポイント (同じ EL) |
| 0x32 | Software Step (lower EL) | シングルステップ |
| 0x33 | Software Step (same EL) | シングルステップ (同じ EL) |
| 0x34 | Watchpoint (lower EL) | ウォッチポイント |
| 0x35 | Watchpoint (same EL) | ウォッチポイント (同じ EL) |
| 0x38 | BKPT (AArch32) | ブレークポイント (32-bit) |
| 0x3C | BRK (AArch64) | ブレークポイント (64-bit) |

#### ISS (Instruction Specific Syndrome) の例

**BRK 命令 (EC = 0x3C) の場合:**

```
ESR_EL2 = 0xf2000000

[31:26]  EC  = 0x3C (BRK instruction)
[25]     IL  = 1    (32-bit instruction)
[24:16]  ISS = 0x00 (reserved)
[15:0]   IMM = 0x00 (BRK の即値 #0)
```

**Data Abort (EC = 0x24) の場合:**

```
ESR_EL2 の ISS フィールド:

[24]     ISV     - Instruction Syndrome Valid
[23:14]  SRT     - Source Register (どのレジスタにロード/ストアしようとしたか)
[13]     SF      - Sixty-Four bit register (64-bit レジスタか)
[12]     AR      - Acquire/Release
[11:10]  FnV     - FAR not Valid (FAR_EL2 が有効か)
[9]      EA      - External Abort
[8]      CM      - Cache Maintenance
[7]      S1PTW   - Stage-2 fault on Stage-1 page table walk
[6]      WnR     - Write not Read (1=書き込み, 0=読み取り)
[5:0]    DFSC    - Data Fault Status Code (ページフォルトの詳細)
```

**DFSC (Data Fault Status Code) の例:**

| DFSC | 意味 |
|------|------|
| 0x04 | Translation fault, level 0 |
| 0x05 | Translation fault, level 1 |
| 0x06 | Translation fault, level 2 |
| 0x07 | Translation fault, level 3 |
| 0x09 | Access flag fault, level 1 |
| 0x0D | Permission fault, level 1 |
| 0x0F | Permission fault, level 3 |

### FAR_EL2 (Fault Address Register)

Data Abort や Instruction Abort の場合、FAR_EL2 には例外を引き起こしたアドレスが記録される。

```
例: ゲストが未マップのアドレス 0x20000 にアクセス

→ VM Exit
   ESR_EL2 = 0x9200004f
     EC   = 0x24 (Data Abort from lower EL)
     ISS  = 0x0000004f
       WnR  = 0 (読み取り)
       DFSC = 0x07 (Translation fault, level 3)
   FAR_EL2 = 0x20000 ← フォルトを起こしたアドレス
```

ハイパーバイザーは、FAR_EL2 を見てどのアドレスがフォルトしたかを知り、動的にメモリをマップしたり、エラーをゲストに返したりできる。

### 今回の VM Exit フロー

```
時間 ─────────────────────────────────────────────────────►

     ホスト                         ゲスト
       │                              │
       │  vcpu.run()                  │
       │ ────────────────────────────►│
       │                              │
       │                              │ mov x0, #42
       │                              │ (X0 = 42 に設定)
       │                              │
       │                              │ brk #0
       │                              │ (例外発生!)
       │                              │
       │◄──────────────────────────── │
       │  VM Exit (EXCEPTION)         │
       │                              │
       │  exit_info を解析            │
       │  EC = 0x3C (BRK)             │
       │  X0 = 42 を確認              │
       │                              │
       ▼                              ▼
     終了
```

### 実行結果の意味

```
VM Exit:
  - Reason: EXCEPTION
  - PC: 0x10004           ← BRK 命令の次のアドレス
  - X0: 42 (0x2a)         ← mov x0, #42 の結果
  - Exception Syndrome: 0xf2000000
  - Exception Class (EC): 0x3c  ← BRK 命令

✓ BRK 命令を検出!
  ゲストが x0 = 42 を設定して BRK を呼び出しました。
```

1. `mov x0, #42` が実行され、X0 レジスタに 42 が格納された
2. `brk #0` が実行され、デバッグ例外が発生
3. VM Exit が発生し、ホスト側に制御が戻った
4. ホスト側で X0 = 42 を確認できた

---

---

## メモリ仮想化の詳細

### 2 段階アドレス変換 (Two-Stage Address Translation)

ARM64 の仮想化では、ゲスト OS のメモリアクセスが 2 段階で変換される。

```
ゲスト仮想アドレス (VA)
         │
         │ Stage-1 Translation (ゲスト OS が管理)
         │ ゲストページテーブル (TTBR0_EL1, TTBR1_EL1)
         ▼
ゲスト物理アドレス (IPA: Intermediate Physical Address)
         │
         │ Stage-2 Translation (ハイパーバイザーが管理)
         │ ホストページテーブル (VTTBR_EL2)
         ▼
ホスト物理アドレス (PA: Physical Address)
```

### Stage-1 Translation (ゲスト OS 管理)

ゲスト OS は、通常の OS と同様に、仮想アドレスを物理アドレス (実際にはゲスト物理アドレス) に変換する。

**使用するレジスタ:**

- `TTBR0_EL1`: ユーザー空間のページテーブルベースアドレス
- `TTBR1_EL1`: カーネル空間のページテーブルベースアドレス
- `TCR_EL1`: Translation Control Register (ページサイズ、アドレス幅など)

**ゲスト OS の視点:**

ゲスト OS は、自分が物理メモリを直接制御していると思っている。しかし、実際には IPA (ゲスト物理アドレス) を操作しているだけで、真の物理アドレスではない。

### Stage-2 Translation (ハイパーバイザー管理)

ハイパーバイザーは、ゲスト物理アドレス (IPA) を真の物理アドレス (PA) に変換する。

**使用するレジスタ:**

- `VTTBR_EL2`: Stage-2 ページテーブルベースアドレス + VMID
- `VTCR_EL2`: Virtualization Translation Control Register (Stage-2 の設定)

**変換の流れ:**

```
1. ゲストが VA 0x400000 にアクセス
   ↓
2. Stage-1: TTBR0_EL1 のページテーブルを参照
   VA 0x400000 → IPA 0x10000 に変換
   ↓
3. Stage-2: VTTBR_EL2 のページテーブルを参照
   IPA 0x10000 → PA 0x12340000 に変換
   ↓
4. 実際の物理メモリ PA 0x12340000 にアクセス
```

### hv_vm_map() の内部動作

`hv_vm_map(guest_addr, host_addr, size, perms)` を呼び出すと、以下の処理が行われる。

```rust
// ユーザーコード
mem.map(0x10000, MemPerms::RWX)?;

// Hypervisor.framework 内部 (カーネル)
hv_vm_map(
    ipa: 0x10000,        // ゲスト物理アドレス
    host_va: 0x7ff8...,  // ホスト仮想アドレス (ユーザープロセスの heap)
    size: 0x1000,
    perms: HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC
)
```

**内部処理:**

1. カーネルが `host_va` をホスト物理アドレス `PA` に変換
2. Stage-2 ページテーブルに `IPA 0x10000 → PA` のマッピングを追加
3. VTTBR_EL2 が指すページテーブルが更新される

**Stage-2 ページテーブルの構造 (簡略化):**

```
VTTBR_EL2
    │
    ▼
┌─────────────────────────────────┐
│  Level 0 Table                  │
│  ┌─────────────────────────┐    │
│  │ Entry 0                 │────┼──► Level 1 Table
│  │ Entry 1                 │    │
│  │ ...                     │    │
│  └─────────────────────────┘    │
└─────────────────────────────────┘
                                   │
                                   ▼
                    ┌─────────────────────────────────┐
                    │  Level 1 Table                  │
                    │  ┌─────────────────────────┐    │
                    │  │ Entry for IPA 0x10000   │    │
                    │  │  → PA 0x12340000        │    │
                    │  │  Permissions: RWX       │    │
                    │  │ Entry for IPA 0x20000   │    │
                    │  │  → PA 0x56780000        │    │
                    │  │ ...                     │    │
                    │  └─────────────────────────┘    │
                    └─────────────────────────────────┘
```

### TLB (Translation Lookaside Buffer)

アドレス変換は高コストな操作のため、CPU は TLB というキャッシュを使用する。

```
┌───────────────────────────────────────────────────┐
│  TLB Entry                                        │
├───────────────────────────────────────────────────┤
│  ASID (Address Space ID)           - 16 bits      │  ← Stage-1 用
│  VMID (Virtual Machine ID)         - 16 bits      │  ← Stage-2 用
│  VA (Virtual Address)              - 64 bits      │
│  IPA (Intermediate Physical Addr)  - 48 bits      │
│  PA (Physical Address)             - 48 bits      │
│  Permissions (R/W/X)               - 3 bits       │
│  Cache attributes                  - 3 bits       │
└───────────────────────────────────────────────────┘
```

**VMID の重要性:**

複数の VM が動作している場合、各 VM に異なる VMID を割り当てる。TLB エントリに VMID が含まれるため、VM 切り替え時に TLB をフラッシュしなくても、正しい変換結果がヒットする。

### 今回のハイパーバイザーでの簡略化

このハイパーバイザーでは、ゲスト OS が独自の仮想メモリ管理を行わないため、Stage-1 translation は実質的に使用されていない。

```
ゲストコードのアドレス 0x10000
         │
         │ Stage-1: 恒等写像 (Identity Mapping) または無効
         ▼
IPA 0x10000
         │
         │ Stage-2: hv_vm_map() で設定
         ▼
ホスト物理アドレス PA 0x12340000
```

ゲストコードは物理アドレスを直接指定しているように見えるが、実際には IPA を指定しており、ハイパーバイザーが Stage-2 で変換している。

### メモリ保護

Stage-2 translation により、ゲストは以下の制約を受ける。

1. **アクセス範囲の制限**: マップされていないアドレスにアクセスすると Data Abort (VM Exit)
2. **権限の制限**: RW でマップされた領域を実行しようとすると Instruction Abort (VM Exit)
3. **ホストメモリの保護**: ゲストは他の VM やホスト OS のメモリにアクセスできない

**例: 未マップ領域へのアクセス**

```rust
// ゲストコードが 0x20000 にアクセス
// しかし、hv_vm_map() で 0x20000 はマップされていない

→ Stage-2 translation で変換失敗
→ Data Abort (同期例外)
→ VM Exit (ExitReason::EXCEPTION)
→ ESR_EL2.EC = 0x24 (Data Abort from lower EL)
```

ハイパーバイザーは、この VM Exit をハンドルして、ゲストにメモリを動的に割り当てたり、エラーを返したりできる。

---

## まとめ

このハイパーバイザーは以下を実証している:

1. **ハードウェア仮想化**: CPU の仮想化支援機能を使用してゲストコードを直接実行
2. **メモリ仮想化**: Stage-2 address translation によるホストメモリのゲストアドレス空間へのマッピング
3. **VM Exit ハンドリング**: ゲストからホストへの制御転送と状態取得
4. **レジスタ操作**: ゲスト CPU レジスタの読み書き
5. **例外処理**: BRK 命令などのデバッグ例外のトラップと解析

### 実装されている技術

- **ARM64 Exception Level**: EL1 (ゲスト) と EL2 (ホスト) の分離
- **2 段階アドレス変換**: Stage-2 translation によるメモリ分離
- **例外ルーティング**: HCR_EL2 による例外のトラップ設定
- **レジスタ仮想化**: ゲスト CPSR/PC の設定と読み取り

### 次のステップ

この基盤の上に、以下を追加することで、本格的な VM（Linux カーネルの起動など）が実現できる。

1. **I/O エミュレーション**: MMIO (Memory-Mapped I/O) のトラップとデバイスエミュレーション
2. **割り込み処理**: vGIC (Virtual Generic Interrupt Controller) の実装
3. **ページング設定**: ゲスト OS が独自のページテーブルを管理できるようにする
4. **複数 vCPU**: マルチコア VM のサポート
5. **デバイスパススルー**: 物理デバイスを直接ゲストに割り当て
