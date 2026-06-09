// ARCH §1 normative dependency rule: "nothing depends on dh-worker".
// Host-runnable (no KVM); uses the cargo that built the test, against the
// workspace lockfile, so it needs no network and fails the moment any
// workspace member (crate, tool, or test package) regrows a dh-worker edge.

use std::process::Command;

#[test]
fn nothing_depends_on_dh_worker() {
    let manifest = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml");
    let out = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--invert",
            "dh-worker",
            "--edges",
            "normal,build,dev",
            "--prefix",
            "none",
            "--manifest-path",
            manifest,
        ])
        .output()
        .expect("failed to run cargo tree");
    assert!(
        out.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let dependents: Vec<&str> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("dh-worker "))
        .collect();
    assert!(
        dependents.is_empty(),
        "ARCH §1 violation — these packages depend on dh-worker: {dependents:?}"
    );
}
