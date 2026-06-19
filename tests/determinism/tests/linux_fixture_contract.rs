//! M9 Linux fixture contract probe.
//!
//! This is intentionally host-only: it validates the staged initramfs before
//! any KVM-heavy Linux gate can spend time booting a known-wrong image.

#![cfg(target_arch = "x86_64")]

mod common;

#[test]
#[ignore = "M9 Linux artifact gate: requires DH_M9_* artifacts but not KVM"]
fn staged_initramfs_satisfies_m9_reference_workload_contract() -> common::TestResult<()> {
    let Some(artifacts) = common::m9_artifacts("linux_fixture_contract")? else {
        return Ok(());
    };

    let contract = common::assert_m9_initramfs_contract(&artifacts.initramfs)?;
    println!(
        "M9 initramfs contract ok: autostart_unit={} exec={} expected_regions={:?}",
        contract.autostart_unit, contract.exec_path, contract.expected_regions
    );
    println!(
        "M9 artifact paths: bzImage={} initramfs={} base_image={} game_image={} image_cache={}",
        artifacts.bzimage.display(),
        artifacts.initramfs.display(),
        artifacts.base_image.display(),
        artifacts.game_image.display(),
        artifacts.image_cache.display()
    );
    Ok(())
}
