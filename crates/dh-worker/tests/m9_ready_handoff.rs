#![cfg(target_arch = "x86_64")]

use std::fs;
use std::process::Command;

use dh_worker::m9_handoff;

#[test]
fn help_prints_usage_to_stdout() {
    let output = Command::new(env!("CARGO_BIN_EXE_dh-m9-ready-handoff"))
        .arg("--help")
        .output()
        .expect("run dh-m9-ready-handoff --help");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("usage: dh-m9-ready-handoff"));
    assert!(output.stderr.is_empty());
}

#[test]
#[ignore = "requires KVM, DH_M9_* artifacts, and an operator-approved private host"]
fn durable_ready_snapshot_handoff_cli_writes_private_env_and_sanitized_summary() {
    let root = tempfile::TempDir::new().expect("tempdir");
    let private_root = root.path().join("private");
    let reference_checkout = root.path().join("reference-workload");
    let workload_manifest = reference_checkout
        .join("dist")
        .join("workload-image-0.1.0")
        .join("workload-image.yaml");
    fs::create_dir_all(workload_manifest.parent().expect("manifest parent"))
        .expect("create reference workload fixture");
    fs::write(&workload_manifest, "name: m9-reference-workload\n")
        .expect("write reference workload manifest fixture");

    let handoff_env = private_root
        .join("rom-bridge-o73")
        .join("handoff")
        .join("bridge-real-restore-snapshot.env");
    let snapstore_config = private_root
        .join("rom-bridge-o73")
        .join("snapstore")
        .join("config.toml");
    let public_summary = private_root
        .join("rom-bridge-o73")
        .join("public-summary.txt");

    let args = vec![
        "--private-root".to_string(),
        private_root.display().to_string(),
        "--snapstore-data-root".to_string(),
        private_root
            .join("rom-bridge-o73")
            .join("snapstore")
            .join("data")
            .display()
            .to_string(),
        "--snapstore-uds".to_string(),
        private_root
            .join("rom-bridge-o73")
            .join("runtime")
            .join("snapstore.sock")
            .display()
            .to_string(),
        "--reference-workload-checkout".to_string(),
        reference_checkout.display().to_string(),
        "--workload-manifest".to_string(),
        workload_manifest.display().to_string(),
        "--bridge-hypervisor-endpoint".to_string(),
        "unix:///run/dh/grpc.sock".to_string(),
        "--bridge-private-root".to_string(),
        private_root.join("bridge").display().to_string(),
        "--bridge-workload-image-ref".to_string(),
        "operator-approved-workload-ref".to_string(),
        "--bridge-capture-spec-ref".to_string(),
        "operator-approved-capture-ref".to_string(),
        "--handoff-env".to_string(),
        handoff_env.display().to_string(),
        "--snapstore-config".to_string(),
        snapstore_config.display().to_string(),
        "--public-summary".to_string(),
        public_summary.display().to_string(),
    ];

    let mut stdout = Vec::new();
    let report = m9_handoff::run_cli(args, &mut stdout).unwrap_or_else(|err| {
        panic!("{}", err.public_message());
    });

    assert_eq!(report.snapshot_ref_hex.len(), 64);
    assert!(report
        .snapshot_ref_hex
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));

    let handoff = fs::read_to_string(&handoff_env).expect("read private handoff env");
    assert!(handoff.contains("BRIDGE_REAL_SNAPSHOT_REF="));
    assert!(handoff.contains("SNAPSTORE_GRPC_UDS_PATH="));
    assert!(!handoff.contains("BRIDGE_CREATE_VM_CONFIG_REF"));

    let summary = fs::read_to_string(&public_summary).expect("read public summary");
    assert_eq!(summary.as_bytes(), stdout.as_slice());
    assert!(summary.contains("RestoreSnapshot verification succeeded: yes"));
    let stdout = std::str::from_utf8(&stdout).expect("stdout is utf-8");
    for forbidden in [
        report.snapshot_ref_hex.as_str(),
        private_root.to_str().expect("private root is utf-8"),
        handoff_env.to_str().expect("handoff path is utf-8"),
        snapstore_config
            .to_str()
            .expect("snapstore config path is utf-8"),
        "operator-approved-workload-ref",
        "operator-approved-capture-ref",
    ] {
        assert!(
            !summary.contains(forbidden),
            "public summary leaked forbidden literal {forbidden}"
        );
        assert!(
            !stdout.contains(forbidden),
            "stdout leaked forbidden literal {forbidden}"
        );
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        assert_eq!(
            fs::metadata(&handoff_env)
                .expect("handoff env metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(&snapstore_config)
                .expect("snapstore config metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
