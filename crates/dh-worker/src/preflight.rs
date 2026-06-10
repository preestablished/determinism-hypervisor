//! `dh-workerd --preflight` (IMPLEMENTATION-PLAN M0): assert the ARCH §7.4
//! host configuration AND the §2.1 KVM capability table before the service
//! takes slots. Fails loudly on a stock kernel config or any missing
//! capability — a worker on a drifted host records unreplayable logs.
//!
//! The §7.4 checks mirror `docs/ops/apply-host-config.sh --verify`
//! (shell, ops-facing); this is the in-process, service-facing authority.
//! Check logic is unit-testable host-side: every check parses an input
//! string and is exercised against good/bad fixtures; only the collectors
//! touch /sys //proc, and the full run is hardware-gated.

use std::fmt;
use std::fs;

/// The slot cores the §7.4 config pins (single source for the service; the
/// ops script's SLOT_CORES is the shell-side mirror of the same decision).
pub const SLOT_CORES: &str = "2-5";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckResult {
    pub name: &'static str,
    pub ok: bool,
    pub got: String,
    pub want: String,
}

impl fmt::Display for CheckResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {:<28} got [{}] want [{}]",
            if self.ok { "ok  " } else { "FAIL" },
            self.name,
            self.got,
            self.want
        )
    }
}

fn check(name: &'static str, got: impl Into<String>, want: impl Into<String>) -> CheckResult {
    let got = got.into();
    let want = want.into();
    CheckResult {
        name,
        ok: got == want,
        got,
        want,
    }
}

fn read_trim(path: &str) -> String {
    fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|e| format!("<unreadable: {e}>"))
}

/// THP mode must be madvise or never (§7.4 item 5).
pub fn check_thp(enabled_line: &str) -> CheckResult {
    let mode = enabled_line
        .split_whitespace()
        .find(|t| t.starts_with('['))
        .unwrap_or("<none>");
    CheckResult {
        name: "thp.mode",
        ok: mode == "[madvise]" || mode == "[never]",
        got: mode.to_string(),
        want: "[madvise] or [never]".to_string(),
    }
}

/// No IRQ's effective affinity may include a slot core (§7.4 item 1b;
/// single-digit CPU ids guaranteed by the 6-CPU host class).
pub fn check_irq_affinity(lists: &[(String, String)]) -> CheckResult {
    let bad: Vec<String> = lists
        .iter()
        .filter(|(_, v)| v.chars().any(|c| ('2'..='5').contains(&c)))
        .map(|(irq, v)| format!("{irq}={v}"))
        .collect();
    CheckResult {
        name: "irq.effective_affinity",
        ok: bad.is_empty(),
        got: if bad.is_empty() {
            "none on slot cores".into()
        } else {
            bad.join(" ")
        },
        want: "none on slot cores".into(),
    }
}

/// All §7.4 host checks against live /sys //proc state.
pub fn host_checks() -> Vec<CheckResult> {
    let mut out = vec![
        check(
            "cpu.isolated",
            read_trim("/sys/devices/system/cpu/isolated"),
            SLOT_CORES,
        ),
        check(
            "cpu.nohz_full",
            read_trim("/sys/devices/system/cpu/nohz_full"),
            SLOT_CORES,
        ),
        check(
            "kvm_intel.ple_gap",
            read_trim("/sys/module/kvm_intel/parameters/ple_gap"),
            "0",
        ),
        check(
            "kvm_intel.pml",
            read_trim("/sys/module/kvm_intel/parameters/pml"),
            "Y",
        ),
        check(
            "nmi_watchdog",
            read_trim("/proc/sys/kernel/nmi_watchdog"),
            "0",
        ),
        check(
            "perf_event_paranoid",
            read_trim("/proc/sys/kernel/perf_event_paranoid"),
            "1",
        ),
    ];
    out.push(check_thp(&read_trim(
        "/sys/kernel/mm/transparent_hugepage/enabled",
    )));

    let mut lists = Vec::new();
    if let Ok(dir) = fs::read_dir("/proc/irq") {
        for entry in dir.flatten() {
            let p = entry.path().join("effective_affinity_list");
            if let Ok(v) = fs::read_to_string(&p) {
                lists.push((
                    entry.file_name().to_string_lossy().into_owned(),
                    v.trim().to_string(),
                ));
            }
        }
    }
    out.push(check_irq_affinity(&lists));
    out
}

/// §2.1 capability gate via dh-vmm (named-missing-caps on failure), plus a
/// slot-VM construction smoke (memfd, NOHUGEPAGE, dirty ring, vCPU).
pub fn kvm_checks() -> Vec<CheckResult> {
    match dh_vmm::kvm::KvmSystem::open() {
        Ok(sys) => {
            let mut out = vec![CheckResult {
                name: "kvm.caps(§2.1)",
                ok: true,
                got: "all present".into(),
                want: "all present".into(),
            }];
            out.push(CheckResult {
                name: "kvm.dirty_ring",
                ok: sys.dirty_ring,
                got: sys.dirty_ring.to_string(),
                want: "true".into(),
            });
            out.push(match sys.create_slot_vm(2 * 1024 * 1024) {
                Ok(_) => CheckResult {
                    name: "kvm.slot_vm_smoke",
                    ok: true,
                    got: "constructed".into(),
                    want: "constructed".into(),
                },
                Err(e) => CheckResult {
                    name: "kvm.slot_vm_smoke",
                    ok: false,
                    got: format!("{e:?}"),
                    want: "constructed".into(),
                },
            });
            out
        }
        Err(e) => vec![CheckResult {
            name: "kvm.caps(§2.1)",
            ok: false,
            got: format!("{e:?}"),
            want: "all present".into(),
        }],
    }
}

/// The full preflight: §7.4 host + §2.1 KVM. Returns all results; the
/// binary prints them and exits nonzero if any failed.
pub fn run_preflight() -> (Vec<CheckResult>, bool) {
    let mut results = host_checks();
    results.extend(kvm_checks());
    let ok = results.iter().all(|r| r.ok);
    (results, ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thp_parser_good_and_bad() {
        assert!(check_thp("always [madvise] never").ok);
        assert!(check_thp("always madvise [never]").ok);
        assert!(!check_thp("[always] madvise never").ok);
        assert!(!check_thp("").ok);
    }

    #[test]
    fn irq_affinity_parser() {
        let good = vec![
            ("9".to_string(), "0-1".to_string()),
            ("14".to_string(), "0".to_string()),
        ];
        assert!(check_irq_affinity(&good).ok);
        let bad = vec![("123".to_string(), "5".to_string())];
        let r = check_irq_affinity(&bad);
        assert!(!r.ok);
        assert!(r.got.contains("123=5"));
    }

    #[test]
    fn check_compares_exactly() {
        assert!(check("x", "0", "0").ok);
        assert!(!check("x", "1", "0").ok);
        assert!(!check("x", "<unreadable: nope>", "0").ok);
    }

    /// HARDWARE-GATED acceptance: the full preflight passes on the §7.4
    /// box (and would fail loudly on a stock config — the parsers above
    /// prove the failure paths).
    #[test]
    fn full_preflight_passes_on_configured_host() {
        if !std::path::Path::new("/dev/kvm").exists() {
            eprintln!("skipping: no /dev/kvm");
            return;
        }
        let (results, ok) = run_preflight();
        for r in &results {
            eprintln!("{r}");
        }
        assert!(ok, "preflight must pass on the configured lab box");
        assert!(results.len() >= 10);
    }
}
