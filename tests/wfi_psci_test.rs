//! WFI/WFE と PSCI ハンドリングのテスト
//!
//! これらのテストは Hypervisor.framework の entitlements が必要です。
//! ローカルで実行する場合は `cargo test --ignored` を使用してください。

use hypervisor::Hypervisor;

/// WFI 命令が正しくハンドリングされ、PC が進むことを確認
#[test]
#[ignore = "requires Hypervisor.framework entitlements (run locally with --ignored)"]
fn wfi_命令が正しくハンドリングされる() {
    let mut hv = Hypervisor::new(0x4000_0000, 0x100_0000).expect("Failed to create hypervisor");

    // WFI 命令: D5 03 20 1F
    // BRK #0: D4 20 00 00
    let instructions: [u32; 2] = [
        0xD503_201F, // WFI
        0xD420_0000, // BRK #0
    ];

    hv.write_instructions(&instructions)
        .expect("Failed to write instructions");

    let result = hv.run(None, None, None).expect("Failed to run");

    // BRK で停止するはず（WFI をスキップして）
    let ec = result
        .exception_syndrome
        .map(|s| (s >> 26) & 0x3f)
        .unwrap_or(0);
    assert_eq!(ec, 0x3c, "Expected BRK exception (EC=0x3c)");

    // PC は BRK 命令の位置（0x4000_0004）
    assert_eq!(result.pc, 0x4000_0004);
}

/// WFE 命令が正しくハンドリングされ、PC が進むことを確認
#[test]
#[ignore = "requires Hypervisor.framework entitlements (run locally with --ignored)"]
fn wfe_命令が正しくハンドリングされる() {
    let mut hv = Hypervisor::new(0x4000_0000, 0x100_0000).expect("Failed to create hypervisor");

    // WFE 命令: D5 03 20 5F
    // BRK #0: D4 20 00 00
    let instructions: [u32; 2] = [
        0xD503_205F, // WFE
        0xD420_0000, // BRK #0
    ];

    hv.write_instructions(&instructions)
        .expect("Failed to write instructions");

    let result = hv.run(None, None, None).expect("Failed to run");

    // BRK で停止するはず（WFE をスキップして）
    let ec = result
        .exception_syndrome
        .map(|s| (s >> 26) & 0x3f)
        .unwrap_or(0);
    assert_eq!(ec, 0x3c, "Expected BRK exception (EC=0x3c)");

    // PC は BRK 命令の位置（0x4000_0004）
    assert_eq!(result.pc, 0x4000_0004);
}

/// HVC 命令で PSCI_VERSION を呼び出し、正しいバージョンを取得
#[test]
#[ignore = "requires Hypervisor.framework entitlements (run locally with --ignored)"]
fn hvc_psci_version_を呼び出せる() {
    let mut hv = Hypervisor::new(0x4000_0000, 0x100_0000).expect("Failed to create hypervisor");

    // MOV X0, #0x84000000 (PSCI_VERSION)
    // HVC #0
    // BRK #0
    let instructions: [u32; 5] = [
        0xD280_0000, // MOV X0, #0x0
        0xF2B0_8000, // MOVK X0, #0x8400, LSL #16 (correct encoding)
        0xD400_0002, // HVC #0
        0xD420_0000, // BRK #0
        0x0000_0000, // padding
    ];

    hv.write_instructions(&instructions)
        .expect("Failed to write instructions");

    // EL1h モード (0x3c5) で実行
    let result = hv.run(Some(0x3c5), None, None).expect("Failed to run");

    // BRK で停止
    let ec = result
        .exception_syndrome
        .map(|s| (s >> 26) & 0x3f)
        .unwrap_or(0);
    assert_eq!(ec, 0x3c, "Expected BRK exception");

    // X0 には PSCI 1.0 (0x0001_0000) が返されるはず
    assert_eq!(
        result.registers[0], 0x0001_0000,
        "Expected PSCI version 1.0"
    );
}

/// HVC で PSCI_FEATURES を呼び出し、VERSION がサポートされていることを確認
#[test]
#[ignore = "requires Hypervisor.framework entitlements (run locally with --ignored)"]
fn hvc_psci_features_でversionがサポートされている() {
    let mut hv = Hypervisor::new(0x4000_0000, 0x100_0000).expect("Failed to create hypervisor");

    // X0 = PSCI_FEATURES (0x8400000A)
    // X1 = PSCI_VERSION (0x84000000)
    // HVC #0
    // BRK #0
    let instructions: [u32; 7] = [
        0xD280_0140, // MOV X0, #0xA
        0xF2B0_8000, // MOVK X0, #0x8400, LSL #16 (correct encoding)
        0xD280_0001, // MOV X1, #0x0
        0xF2B0_8001, // MOVK X1, #0x8400, LSL #16 (correct encoding)
        0xD400_0002, // HVC #0
        0xD420_0000, // BRK #0
        0x0000_0000, // padding
    ];

    hv.write_instructions(&instructions)
        .expect("Failed to write instructions");

    // EL1h モード (0x3c5) で実行
    let result = hv.run(Some(0x3c5), None, None).expect("Failed to run");

    // BRK で停止
    let ec = result
        .exception_syndrome
        .map(|s| (s >> 26) & 0x3f)
        .unwrap_or(0);
    assert_eq!(ec, 0x3c, "Expected BRK exception");

    // X0 = 0 (PSCI_SUCCESS = サポートされている)
    assert_eq!(result.registers[0], 0, "Expected PSCI_SUCCESS (supported)");
}

/// HVC で PSCI_CPU_ON を呼び出し、ALREADY_ON を取得
#[test]
#[ignore = "requires Hypervisor.framework entitlements (run locally with --ignored)"]
fn hvc_psci_cpu_on_はalready_onを返す() {
    let mut hv = Hypervisor::new(0x4000_0000, 0x100_0000).expect("Failed to create hypervisor");

    // X0 = PSCI_CPU_ON (0xC4000003)
    // X1 = target_cpu = 0
    // X2 = entry_point = 0
    // X3 = context_id = 0
    // HVC #0
    // BRK #0
    let instructions: [u32; 8] = [
        0xD280_0060, // MOV X0, #0x3
        0xF2B8_8000, // MOVK X0, #0xC400, LSL #16
        0xD280_0001, // MOV X1, #0
        0xD280_0002, // MOV X2, #0
        0xD280_0003, // MOV X3, #0
        0xD400_0002, // HVC #0
        0xD420_0000, // BRK #0
        0x0000_0000, // padding
    ];

    hv.write_instructions(&instructions)
        .expect("Failed to write instructions");

    // EL1h モード (0x3c5) で実行
    let result = hv.run(Some(0x3c5), None, None).expect("Failed to run");

    // BRK で停止
    let ec = result
        .exception_syndrome
        .map(|s| (s >> 26) & 0x3f)
        .unwrap_or(0);
    assert_eq!(ec, 0x3c, "Expected BRK exception");

    // X0 = PSCI_E_ALREADY_ON (-4)
    assert_eq!(
        result.registers[0], 0xFFFF_FFFF_FFFF_FFFC,
        "Expected PSCI_E_ALREADY_ON"
    );
}

/// HVC で未知の PSCI 関数を呼び出し、NOT_SUPPORTED を取得
#[test]
#[ignore = "requires Hypervisor.framework entitlements (run locally with --ignored)"]
fn hvc_未知の関数はnot_supportedを返す() {
    let mut hv = Hypervisor::new(0x4000_0000, 0x100_0000).expect("Failed to create hypervisor");

    // X0 = 未知の関数 (0xFFFFFFFF)
    // HVC #0
    // BRK #0
    let instructions: [u32; 5] = [
        0x9280_0000, // MOV X0, #-1 (0xFFFFFFFF_FFFFFFFF)
        0xD400_0002, // HVC #0
        0xD420_0000, // BRK #0
        0x0000_0000, // padding
        0x0000_0000, // padding
    ];

    hv.write_instructions(&instructions)
        .expect("Failed to write instructions");

    // EL1h モード (0x3c5) で実行
    let result = hv.run(Some(0x3c5), None, None).expect("Failed to run");

    // BRK で停止
    let ec = result
        .exception_syndrome
        .map(|s| (s >> 26) & 0x3f)
        .unwrap_or(0);
    assert_eq!(ec, 0x3c, "Expected BRK exception");

    // X0 = PSCI_E_NOT_SUPPORTED (-1)
    assert_eq!(
        result.registers[0], 0xFFFF_FFFF_FFFF_FFFF,
        "Expected PSCI_E_NOT_SUPPORTED"
    );
}
