//! Earlycon (UART 出力) テスト
//!
//! UART PL011 への出力が正しく動作することを確認するテスト。
//! 実際の Linux カーネルの earlycon ドライバと同様の動作をテストする。

use hypervisor::devices::uart::Pl011Uart;
use hypervisor::mmio::MmioHandler;
use hypervisor::Hypervisor;

/// UART のベースアドレス
const UART_BASE: u64 = 0x0900_0000;

/// UART レジスタオフセット
const UART_DR: u64 = 0x00;
const UART_FR: u64 = 0x18;
const UART_CR: u64 = 0x30;

/// Flag Register bits
const FR_TXFE: u64 = 1 << 7; // TX FIFO Empty

/// Control Register bits
const CR_UARTEN: u64 = 1 << 0;
const CR_TXE: u64 = 1 << 8;

/// UART に 1 文字出力する ARM64 命令列を生成
///
/// 入力: X0 = 出力する文字
/// 処理: UART_FR を読んで TXFE を確認し、UART_DR に書き込む
fn generate_uart_putchar_instructions(uart_base: u64) -> Vec<u32> {
    // UART ベースアドレスを X1 に設定
    let base_lo = (uart_base & 0xFFFF) as u32;
    let base_hi = ((uart_base >> 16) & 0xFFFF) as u32;

    vec![
        // X1 = UART_BASE
        0xD280_0001 | (base_lo << 5), // MOV X1, #base_lo
        0xF2A0_0001 | (base_hi << 5), // MOVK X1, #base_hi, LSL #16
        // X2 = UART_FR offset
        0xD280_0302, // MOV X2, #0x18
        // X3 = UART_DR offset
        0xD280_0003, // MOV X3, #0x0
        // UART_DR に書き込み (str w0, [x1, x3])
        0xB823_0020, // str w0, [x1, x3]
        // BRK で停止
        0xD420_0000, // BRK #0
    ]
}

/// UART に文字列を出力する ARM64 命令列を生成
///
/// 文字列データは命令列の後に配置され、相対アドレスで参照される。
fn generate_uart_puts_instructions(uart_base: u64, message: &str) -> Vec<u32> {
    let base_lo = (uart_base & 0xFFFF) as u32;
    let base_hi = ((uart_base >> 16) & 0xFFFF) as u32;

    let mut instructions = vec![
        // X1 = UART_BASE
        0xD280_0001 | (base_lo << 5), // MOV X1, #base_lo
        0xF2A0_0001 | (base_hi << 5), // MOVK X1, #base_hi, LSL #16
        // X2 = 文字列へのオフセット (命令数 * 4 + PC)
        // ADR X2, string_data (PC 相対)
        // 命令数: 7 (このブロック) + 文字列データの開始位置
    ];

    // 文字列の長さ
    let msg_len = message.len();

    // ループ: 各文字を UART に出力
    // X3 = カウンタ (0 から開始)
    // X4 = 文字列長
    instructions.extend([
        0xD280_0003,                            // MOV X3, #0 (カウンタ)
        0xD280_0004 | ((msg_len as u32) << 5),  // MOV X4, #msg_len
        // loop:
        0xEB04_007F, // CMP X3, X4
        0x5400_00A0, // B.EQ end (offset = 5 instructions = 20 bytes)
        // ADR X5, string_data
        0x1000_0085, // ADR X5, .+16 (4 instructions ahead)
        // X0 = string[X3]
        0x3863_6CA0, // LDRB W0, [X5, X3]
        // str w0, [x1] - UART_DR に書き込み
        0xB900_0020, // STR W0, [X1]
        // X3++
        0x9100_0463, // ADD X3, X3, #1
        // B loop
        0x17FF_FFFA, // B loop (offset = -6 instructions = -24 bytes)
        // end:
        0xD420_0000, // BRK #0
    ]);

    // 文字列データを命令として追加 (4 バイト境界)
    let msg_bytes = message.as_bytes();
    for chunk in msg_bytes.chunks(4) {
        let mut word = 0u32;
        for (i, &byte) in chunk.iter().enumerate() {
            word |= (byte as u32) << (i * 8);
        }
        instructions.push(word);
    }

    instructions
}

/// 簡単な UART 出力テスト（単一文字）
#[test]
#[ignore = "requires Hypervisor.framework entitlements (run locally with --ignored)"]
fn uart_に1文字出力できる() {
    let guest_addr = 0x4000_0000u64;
    let mut hv = Hypervisor::new(guest_addr, 0x1000_0000).expect("Failed to create hypervisor");

    // UART デバイスを登録
    let uart = Pl011Uart::new(UART_BASE);
    hv.register_mmio_handler(Box::new(uart));

    // 'A' を UART に出力する命令
    // MOVZ encoding: sf=1, opc=10, 100101, hw, imm16, Rd
    // MOVZ X0, #0x41 = 0xD280_0820
    // MOVZ X1, #0x0900, LSL #16 = 0xD2A1_2001
    let instructions: [u32; 5] = [
        0xD280_0820, // MOVZ X0, #0x41 ('A')
        0xD2A1_2001, // MOVZ X1, #0x0900, LSL #16 (UART_BASE = 0x0900_0000)
        0xB900_0020, // STR W0, [X1]
        0xD420_0000, // BRK #0
        0x0000_0000, // padding
    ];

    hv.write_instructions(&instructions)
        .expect("Failed to write instructions");

    let result = hv.run(None, None, None).expect("Failed to run");

    // BRK で停止したことを確認
    let ec = result
        .exception_syndrome
        .map(|s| (s >> 26) & 0x3f)
        .unwrap_or(0);
    assert_eq!(ec, 0x3c, "Expected BRK exception");
}

/// UART の Flag Register を読めることを確認
#[test]
#[ignore = "requires Hypervisor.framework entitlements (run locally with --ignored)"]
fn uart_flag_registerを読める() {
    let guest_addr = 0x4000_0000u64;
    let mut hv = Hypervisor::new(guest_addr, 0x1000_0000).expect("Failed to create hypervisor");

    // UART デバイスを登録
    let uart = Pl011Uart::new(UART_BASE);
    hv.register_mmio_handler(Box::new(uart));

    // UART_FR を読み取る命令
    // MOVZ X1, #0x0900, LSL #16 = 0xD2A1_2001
    // ADD X1, X1, #0x18 (FR offset)
    // LDR W0, [X1]
    let instructions: [u32; 5] = [
        0xD2A1_2001, // MOVZ X1, #0x0900, LSL #16 (UART_BASE)
        0x9100_6021, // ADD X1, X1, #0x18 (FR offset)
        0xB940_0020, // LDR W0, [X1]
        0xD420_0000, // BRK #0
        0x0000_0000, // padding
    ];

    hv.write_instructions(&instructions)
        .expect("Failed to write instructions");

    let result = hv.run(None, None, None).expect("Failed to run");

    // BRK で停止したことを確認
    let ec = result
        .exception_syndrome
        .map(|s| (s >> 26) & 0x3f)
        .unwrap_or(0);
    assert_eq!(ec, 0x3c, "Expected BRK exception");

    // X0 に TXFE フラグが含まれていることを確認
    assert_ne!(
        result.registers[0] & FR_TXFE,
        0,
        "Expected TXFE flag to be set"
    );
}

/// UART の Control Register を読み書きできることを確認
#[test]
#[ignore = "requires Hypervisor.framework entitlements (run locally with --ignored)"]
fn uart_control_registerを読み書きできる() {
    let guest_addr = 0x4000_0000u64;
    let mut hv = Hypervisor::new(guest_addr, 0x1000_0000).expect("Failed to create hypervisor");

    // UART デバイスを登録
    let uart = Pl011Uart::new(UART_BASE);
    hv.register_mmio_handler(Box::new(uart));

    // UART_CR に書き込み、読み取りする命令
    // X0 = CR_UARTEN | CR_TXE | CR_RXE = 0x301
    // X1 = UART_BASE + 0x30
    // str w0, [x1]; ldr w2, [x1]
    let cr_value = CR_UARTEN | CR_TXE | (1 << 9); // UARTEN | TXE | RXE = 0x301
    let instructions: [u32; 7] = [
        0xD280_0000 | ((cr_value as u32 & 0xFFFF) << 5), // MOVZ X0, #cr_value
        0xD2A1_2001,                                     // MOVZ X1, #0x0900, LSL #16 (UART_BASE)
        0x9100_C021,                                     // ADD X1, X1, #0x30 (CR offset)
        0xB900_0020,                                     // STR W0, [X1]
        0xB940_0022,                                     // LDR W2, [X1]
        0xD420_0000,                                     // BRK #0
        0x0000_0000,                                     // padding
    ];

    hv.write_instructions(&instructions)
        .expect("Failed to write instructions");

    let result = hv.run(None, None, None).expect("Failed to run");

    // BRK で停止したことを確認
    let ec = result
        .exception_syndrome
        .map(|s| (s >> 26) & 0x3f)
        .unwrap_or(0);
    assert_eq!(ec, 0x3c, "Expected BRK exception");

    // X2 に書き込んだ値が読み取れることを確認
    assert_eq!(
        result.registers[2] & 0xFFFF,
        cr_value,
        "Expected CR value to match"
    );
}

/// Unit test: Pl011Uart の直接テスト
#[test]
fn uart_earlycon_シーケンスが動作する() {
    let mut uart = Pl011Uart::new(UART_BASE);

    // 1. Flag Register を読んで TXFE を確認
    let fr = uart.read(UART_FR, 4).unwrap();
    assert_ne!(fr & FR_TXFE, 0, "TX FIFO should be empty");

    // 2. Control Register を設定 (UART enable, TX enable)
    uart.write(UART_CR, CR_UARTEN | CR_TXE, 4).unwrap();

    // 3. 文字列 "Hello" を出力
    for ch in b"Hello" {
        // TXFE を確認 (常に empty なので省略可能)
        let fr = uart.read(UART_FR, 4).unwrap();
        assert_ne!(fr & FR_TXFE, 0);

        // 文字を出力
        uart.write(UART_DR, *ch as u64, 4).unwrap();
    }
}

/// Unit test: 連続した UART 出力
#[test]
fn uart_連続出力が動作する() {
    let mut uart = Pl011Uart::new(UART_BASE);

    // Linux earlycon と同様のシーケンス
    let message = "Booting Linux...\n";

    for ch in message.bytes() {
        // TX FIFO が空になるまで待つ (エミュレーションでは常に空)
        loop {
            let fr = uart.read(UART_FR, 4).unwrap();
            if (fr & FR_TXFE) != 0 {
                break;
            }
        }
        // 文字を出力
        uart.write(UART_DR, ch as u64, 4).unwrap();
    }
}
