#![deny(unsafe_code)] // targeted allows in the x86_64 KVM modules only (raw ioctls, memfd, madvise)

// Architecture split (bead v5w): the pure determinism math — agenda
// scheduling, virtual-time rationals, machine config, block fixtures —
// builds and tests everywhere (the aarch64 CI lane covers it). The
// KVM-touching modules consume x86_64-only kvm-bindings APIs (MSR
// filter, CPUID, VMX-shaped exits) and are gated to x86_64.
pub mod agenda;
pub mod blkfile;
pub mod config;
pub mod vt;
// Pure byte transform (ARCH §8.1 XSAVE canonicalization): host-runnable
// everywhere; only its CPUID/KVM helpers are x86-gated internally.
pub mod xsave;

#[cfg(target_arch = "x86_64")]
pub mod boot;
#[cfg(target_arch = "x86_64")]
pub mod boundary;
#[cfg(target_arch = "x86_64")]
pub mod cpuid;
#[cfg(target_arch = "x86_64")]
pub mod dirty;
#[cfg(target_arch = "x86_64")]
pub mod hash;
#[cfg(target_arch = "x86_64")]
pub mod inject;
#[cfg(target_arch = "x86_64")]
pub mod kvm;
#[cfg(target_arch = "x86_64")]
pub mod msr;
#[cfg(target_arch = "x86_64")]
pub mod run;
#[cfg(target_arch = "x86_64")]
pub mod runctl;
#[cfg(target_arch = "x86_64")]
pub mod tsc;
#[cfg(target_arch = "x86_64")]
pub mod vcpu_state;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotState {
    Empty,
    Running,
    Paused,
    Frozen,
}

/// Slot-state machine violation (risk R9). Loud by design: a write-path
/// call on a Frozen slot is the software half of the fork-parent guard —
/// the memfd seal (kvm.rs `SlotVm::freeze_ram`) blocks NEW writable
/// mappings, but the parent's own existing KVM mapping stays writable
/// (F_SEAL_WRITE is unavailable while it exists, ARCH §8.4), so THIS check
/// is the only thing standing between a stray engine call and corrupting
/// every CoW child's shared baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotStateError {
    InvalidTransition {
        from: SlotState,
        to: SlotState,
    },
    /// A write-path API was invoked on a Frozen slot (R9).
    FrozenWriteDenied {
        api: &'static str,
    },
    /// A write-path API was invoked on an Empty slot.
    EmptyWriteDenied {
        api: &'static str,
    },
}

impl SlotState {
    /// The §2.2/§8.4 transition relation:
    ///
    /// - `Empty → Paused`: CreateVm / RestoreSnapshot land paused.
    /// - `Paused ⇄ Running`: Run / Pause (boundary engine).
    /// - `Paused → Frozen`: Fork freezes the parent (§8.4).
    /// - `Frozen → Paused`: freeing the last child unfreezes the parent
    ///   (§2.2 DestroyVm semantics).
    /// - `Paused → Empty`, `Frozen → Empty`: DestroyVm. A Running slot must
    ///   pause first — destroying mid-run has no deterministic boundary.
    ///
    /// Everything else (including self-transitions) is a state-machine bug
    /// and errors loudly rather than being silently absorbed.
    pub fn can_transition(self, to: SlotState) -> bool {
        use SlotState::*;
        matches!(
            (self, to),
            (Empty, Paused)
                | (Paused, Running)
                | (Running, Paused)
                | (Paused, Frozen)
                | (Frozen, Paused)
                | (Paused, Empty)
                | (Frozen, Empty)
        )
    }

    pub fn transition(self, to: SlotState) -> Result<SlotState, SlotStateError> {
        if self.can_transition(to) {
            Ok(to)
        } else {
            Err(SlotStateError::InvalidTransition { from: self, to })
        }
    }

    /// Gate every API that can mutate guest RAM, vCPU state, or device
    /// state (run, inject, restore-into, MMIO dispatch, RAM writes).
    /// `api` names the caller for the loud error. Paused and Running slots
    /// are writable; Frozen is the R9 denial; Empty has nothing to write.
    ///
    /// INTEGRATION (not yet wired): no engine code calls this yet — the
    /// slot table (bead ol1), CoW fork (9e4) and snapshot engine (qmp) are
    /// the intended call sites. If those land without adopting this guard,
    /// that is a review failure, not a design change.
    pub fn ensure_write_path(self, api: &'static str) -> Result<(), SlotStateError> {
        match self {
            SlotState::Paused | SlotState::Running => Ok(()),
            SlotState::Frozen => Err(SlotStateError::FrozenWriteDenied { api }),
            SlotState::Empty => Err(SlotStateError::EmptyWriteDenied { api }),
        }
    }
}

pub fn initial_slot_state() -> SlotState {
    SlotState::Empty
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KvmCaps {
    pub user_space_msr: bool,
    pub msr_filter: bool,
    pub dirty_ring: bool,
    pub immediate_exit: bool,
    pub no_in_kernel_irqchip: bool,
}

pub fn required_caps_template() -> KvmCaps {
    KvmCaps {
        user_space_msr: true,
        msr_filter: true,
        dirty_ring: true,
        immediate_exit: true,
        no_in_kernel_irqchip: true,
    }
}

pub fn missing_caps(caps: &KvmCaps) -> Vec<&'static str> {
    [
        (caps.user_space_msr, "KVM_CAP_X86_USER_SPACE_MSR"),
        (caps.msr_filter, "KVM_MSR_EXIT_REASON_FILTER"),
        (caps.dirty_ring, "KVM_CAP_DIRTY_LOG_RING"),
        (caps.immediate_exit, "KVM_CAP_IMMEDIATE_EXIT"),
        (caps.no_in_kernel_irqchip, "no in-kernel irqchip"),
    ]
    .into_iter()
    .filter_map(|(present, name)| (!present).then_some(name))
    .collect()
}

// Single source of the M0 preflight summary line: dh-worker's preflight and
// dh-cli both print it, but neither may define it — dh-cli must not depend on
// dh-worker (ARCH §1), so the shared format lives here.
pub fn m0_missing_caps_summary() -> String {
    format!(
        "kvm_m0_missing_caps={}",
        missing_caps(&required_caps_template()).len()
    )
}

pub fn architecture_components_present() -> bool {
    dh_detclock::DET_CLOCK_COMPONENT == "dh-detclock"
        && dh_devices::DEVICE_MODEL_COMPONENT == "dh-devices"
        && dh_inputlog::DHILOG_FORMAT_VERSION == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_satisfies_m0_caps() {
        assert!(missing_caps(&required_caps_template()).is_empty());
    }

    #[test]
    fn arch_components_are_linked() {
        assert!(architecture_components_present());
    }

    #[test]
    fn m0_summary_format_is_stable() {
        assert_eq!(m0_missing_caps_summary(), "kvm_m0_missing_caps=0");
    }
}

#[cfg(test)]
mod slot_state_tests {
    use super::SlotState::*;
    use super::*;

    const ALL: [SlotState; 4] = [Empty, Running, Paused, Frozen];

    #[test]
    fn transition_matrix_is_exactly_the_spec_relation() {
        let allowed = [
            (Empty, Paused),
            (Paused, Running),
            (Running, Paused),
            (Paused, Frozen),
            (Frozen, Paused),
            (Paused, Empty),
            (Frozen, Empty),
        ];
        for from in ALL {
            for to in ALL {
                let expect = allowed.contains(&(from, to));
                assert_eq!(
                    from.can_transition(to),
                    expect,
                    "transition {from:?} -> {to:?}: expected allowed={expect}"
                );
                match from.transition(to) {
                    Ok(next) => {
                        assert!(expect);
                        assert_eq!(next, to);
                    }
                    Err(SlotStateError::InvalidTransition { from: f, to: t }) => {
                        assert!(!expect);
                        assert_eq!((f, t), (from, to));
                    }
                    Err(other) => panic!("wrong error kind: {other:?}"),
                }
            }
        }
    }

    #[test]
    fn no_self_transitions() {
        for s in ALL {
            assert!(!s.can_transition(s), "{s:?} -> {s:?} must be rejected");
        }
    }

    #[test]
    fn running_cannot_be_destroyed_or_frozen_directly() {
        assert!(!Running.can_transition(Empty)); // pause first
        assert!(!Running.can_transition(Frozen)); // fork requires Paused parent
    }

    #[test]
    fn frozen_denies_every_write_path_loudly() {
        assert_eq!(
            Frozen.ensure_write_path("inject_inputs"),
            Err(SlotStateError::FrozenWriteDenied {
                api: "inject_inputs"
            })
        );
        assert_eq!(
            Empty.ensure_write_path("run"),
            Err(SlotStateError::EmptyWriteDenied { api: "run" })
        );
        assert_eq!(Paused.ensure_write_path("restore"), Ok(()));
        assert_eq!(Running.ensure_write_path("mmio"), Ok(()));
    }

    #[test]
    fn fork_lifecycle_walk() {
        // Empty -> Paused -> Frozen (fork) -> Paused (last child freed)
        // -> Frozen (re-fork) -> Empty (destroy parent).
        let mut s = initial_slot_state();
        for to in [Paused, Frozen, Paused, Frozen, Empty] {
            s = s.transition(to).unwrap();
        }
        assert_eq!(s, Empty);
    }
}
