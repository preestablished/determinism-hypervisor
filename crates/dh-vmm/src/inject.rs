//! Deterministic interrupt injection (ARCH §3.4): deliver vector `v` at
//! the FIRST injectable boundary ≥ B.
//!
//! Injectability is a pure function of guest state —
//! `ready_for_interrupt_injection`, the guest IF flag, no pending/injected
//! exception, no interrupt shadow — so every replay defers by the
//! identical number of instructions. The engine lands (§3.2), checks, and
//! if the window is closed single-steps forward re-checking at every
//! retirement. The stepped re-check IS the deferral mechanism — §3.4's
//! `request_interrupt_window` flag belongs to a future full-run deferral
//! optimization and is deliberately NOT set here: with the flag armed, a
//! mid-deferral STI makes KVM exit IrqWindowOpen instead of Debug and the
//! landing loop would treat the §3.4 success case as a foreign exit
//! (live-reproduced in review). IrqWindowOpen is still swallowed as
//! benign during deferral, defensively.
//!
//! KVM_INTERRUPT queues the vector; KVM delivers it on the next KVM_RUN
//! entry BEFORE any guest instruction retires — so `delivered_icount` is
//! the boundary at queue time, and that is what run control records in
//! the AUX `TIMER_FIRE.delivered_icount` field for verification.

use std::os::fd::AsRawFd;

use kvm_bindings::kvm_interrupt;
use kvm_ioctls::{VcpuExit, VcpuFd};
use vmm_sys_util::ioctl::ioctl_with_ref;
use vmm_sys_util::ioctl_iow_nr;

use dh_detclock::counter::InstRetired;

use crate::boundary::{Boundary, BoundaryError, Margins, land_at};

// kvm-ioctls 0.24 does not wrap KVM_INTERRUPT (userspace-irqchip vector
// queue); raw ioctl per the msr.rs pattern.
ioctl_iow_nr!(KVM_INTERRUPT, 0xAE, 0x86, kvm_interrupt);

/// A completed injection, for the run-control AUX record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Injection {
    pub vector: u8,
    /// The boundary the injection was REQUESTED at (B).
    pub requested_icount: u64,
    /// The first injectable boundary ≥ B — where the vector actually
    /// queues (AUX TIMER_FIRE.delivered_icount).
    pub delivered_icount: u64,
    pub delivered_rip: u64,
}

#[derive(Debug)]
pub enum InjectError {
    Boundary(BoundaryError),
    Kvm(String),
    /// The guest never opened an interrupt window within the deferral
    /// budget. Deterministic (same guest, same count) and loud — run
    /// control decides whether that is a guest-contract violation.
    WindowNeverOpened {
        stepped: u64,
    },
    /// Vectors 0..=31 are exception vectors, never external interrupts.
    BadVector {
        vector: u8,
    },
}

impl std::fmt::Display for InjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InjectError::Boundary(e) => write!(f, "landing: {e}"),
            InjectError::Kvm(e) => write!(f, "kvm: {e}"),
            InjectError::WindowNeverOpened { stepped } => {
                write!(f, "interrupt window never opened within {stepped} steps")
            }
            InjectError::BadVector { vector } => {
                write!(f, "vector {vector} is an exception vector (0..=31)")
            }
        }
    }
}

/// §3.4 step 2: is the vCPU injectable RIGHT NOW (at the current exit
/// boundary)? Pure function of guest state.
pub fn injectable(vcpu: &mut VcpuFd) -> Result<bool, InjectError> {
    let run = vcpu.get_kvm_run();
    if run.ready_for_interrupt_injection == 0 || run.if_flag == 0 {
        return Ok(false);
    }
    let ev = vcpu
        .get_vcpu_events()
        .map_err(|e| InjectError::Kvm(format!("KVM_GET_VCPU_EVENTS: {e}")))?;
    Ok(ev.exception.pending == 0 && ev.exception.injected == 0 && ev.interrupt.shadow == 0)
}

/// Queue vector `v` for delivery on the next KVM_RUN entry (KVM_INTERRUPT,
/// userspace irqchip). Caller has verified injectability.
///
/// CONTRACT: one queued vector per VM entry — a second KVM_INTERRUPT
/// before the next KVM_RUN silently OVERWRITES the first on this kernel
/// (no EEXIST; verified live in review). Run control must enter the guest
/// between injections.
pub fn queue_interrupt(vcpu: &VcpuFd, vector: u8) -> Result<(), InjectError> {
    if vector < 32 {
        // 0..=31 are exception vectors: never injectable as external
        // interrupts (KVM accepts them unvalidated — refuse loudly here).
        return Err(InjectError::BadVector { vector });
    }
    let irq = kvm_interrupt {
        irq: u32::from(vector),
    };
    // SAFETY: valid vCPU fd; the kernel copies the struct during the ioctl.
    #[allow(unsafe_code)]
    let rc = unsafe { ioctl_with_ref(&vcpu.as_raw_fd(), KVM_INTERRUPT(), &irq) };
    if rc != 0 {
        return Err(InjectError::Kvm(format!(
            "KVM_INTERRUPT({vector}): {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

/// §3.4 steps 2–4, starting from the landed boundary `at` (the caller just
/// landed there via §3.2): inject `vector` at the first injectable
/// boundary ≥ `at`, single-stepping the deferral with re-checks at every
/// retirement. `max_defer_steps` bounds a guest that never opens the
/// window (deterministic loud failure; run control owns the verdict).
pub fn inject_at_boundary(
    vcpu: &mut VcpuFd,
    counter: &InstRetired,
    vector: u8,
    at: &Boundary,
    margins: &Margins,
    max_defer_steps: u64,
    on_exit: &mut dyn FnMut(VcpuExit) -> Result<(), BoundaryError>,
) -> Result<Injection, InjectError> {
    let mut current = *at;
    let mut stepped = 0u64;
    loop {
        if injectable(vcpu)? {
            queue_interrupt(vcpu, vector)?;
            break Ok(Injection {
                vector,
                requested_icount: at.icount,
                delivered_icount: current.icount,
                delivered_rip: current.rip,
            });
        }
        if stepped >= max_defer_steps {
            break Err(InjectError::WindowNeverOpened { stepped });
        }
        // Deferral step. IrqWindowOpen is OUR artifact (the window we are
        // polling for) — benign; everything else is the caller's.
        let mut wrapped = |exit: VcpuExit| match exit {
            VcpuExit::IrqWindowOpen => Ok(()),
            other => on_exit(other),
        };
        current = land_at(vcpu, counter, current.icount + 1, margins, &mut wrapped)
            .map_err(InjectError::Boundary)?;
        stepped += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot::load_and_enter;
    use crate::boundary::Margins;
    use crate::kvm::KvmSystem;
    use crate::run::install_kick_handler;
    use dh_detclock::counter::NEVER_FIRES_PERIOD;

    fn gettid() -> i32 {
        // SAFETY: argless syscall.
        #[allow(unsafe_code)]
        unsafe {
            libc::syscall(libc::SYS_gettid) as i32
        }
    }

    fn rig() -> Option<(crate::kvm::SlotVm, InstRetired)> {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return None;
        }
        install_kick_handler().unwrap();
        let sys = KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(16 << 20).unwrap();
        load_and_enter(&slot, nanokernel::landing_loop_elf(), b"1000000000").unwrap();
        let counter = InstRetired::open_for_current_thread().unwrap();
        counter
            .route_overflow_to_thread(gettid(), crate::run::kick_signal())
            .unwrap();
        counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
        counter.reset().unwrap();
        counter.enable().unwrap();
        Some((slot, counter))
    }

    fn no_exits(exit: VcpuExit) -> Result<(), BoundaryError> {
        Err(BoundaryError::Exit(format!("unexpected exit: {exit:?}")))
    }

    /// The boot path enters with RFLAGS = 2 (IF clear) and the landing
    /// loop never executes STI — the window must stay closed and the
    /// deferral must be bounded, loud, and deterministic.
    #[test]
    fn closed_window_defers_deterministically_live() {
        let run = || {
            let (mut slot, counter) = rig()?;
            let b = land_at(
                &mut slot.vcpu,
                &counter,
                10_000,
                &Margins::default(),
                &mut no_exits,
            )
            .unwrap();
            assert!(!injectable(&mut slot.vcpu).unwrap(), "IF=0 guest");
            let err = inject_at_boundary(
                &mut slot.vcpu,
                &counter,
                0x30,
                &b,
                &Margins::default(),
                250,
                &mut no_exits,
            )
            .unwrap_err();
            let InjectError::WindowNeverOpened { stepped } = err else {
                panic!("expected WindowNeverOpened, got {err:?}");
            };
            Some((stepped, counter.read().unwrap()))
        };
        let (Some(a), Some(b)) = (run(), run()) else {
            return;
        };
        assert_eq!(a, b, "deferral must replay identically");
        assert_eq!(a.0, 250);
        assert_eq!(a.1, 10_000 + 250, "one retirement per deferral step");
    }

    #[test]
    fn linux_cpu_compat_interrupt_deferral_is_guest_state_only_live() {
        let run = || {
            let (mut slot, counter) = rig()?;
            let b = land_at(
                &mut slot.vcpu,
                &counter,
                2_000,
                &Margins::default(),
                &mut no_exits,
            )
            .unwrap();
            assert!(!injectable(&mut slot.vcpu).unwrap(), "IF=0 guest");
            let err = inject_at_boundary(
                &mut slot.vcpu,
                &counter,
                0x41,
                &b,
                &Margins::default(),
                32,
                &mut no_exits,
            )
            .unwrap_err();
            let InjectError::WindowNeverOpened { stepped } = err else {
                panic!("expected WindowNeverOpened, got {err:?}");
            };
            Some((stepped, counter.read().unwrap()))
        };
        let (Some(a), Some(b)) = (run(), run()) else {
            return;
        };
        assert_eq!(a, b, "closed-window deferral must replay identically");
        assert_eq!(a, (32, 2_032));
    }

    /// Force the window open (test-only: set IF in RFLAGS at a boundary,
    /// step once so kvm_run.if_flag refreshes) and prove the vector
    /// QUEUES and DELIVERS on the next entry: with an empty IDT the
    /// delivery attempt is a deterministic triple fault (Shutdown exit) —
    /// the guest would otherwise spin in its loop forever.
    #[test]
    fn open_window_injects_and_delivers_live() {
        let Some((mut slot, counter)) = rig() else {
            return;
        };
        let b = land_at(
            &mut slot.vcpu,
            &counter,
            5_000,
            &Margins::default(),
            &mut no_exits,
        )
        .unwrap();

        let mut regs = slot.vcpu.get_regs().unwrap();
        regs.rflags |= 1 << 9; // IF
        slot.vcpu.set_regs(&regs).unwrap();
        // One step refreshes kvm_run's exit-time summary fields.
        let b2 = land_at(
            &mut slot.vcpu,
            &counter,
            b.icount + 1,
            &Margins::default(),
            &mut no_exits,
        )
        .unwrap();

        let inj = inject_at_boundary(
            &mut slot.vcpu,
            &counter,
            0x30,
            &b2,
            &Margins::default(),
            16,
            &mut no_exits,
        )
        .unwrap();
        assert_eq!(
            inj.delivered_icount, b2.icount,
            "window already open: delivery at the requested boundary"
        );

        // Delivery through an empty IDT: deterministic triple fault on the
        // very next entry — observed as Shutdown, never a retired guest
        // instruction (the landing loop would otherwise just keep going).
        let mut saw_shutdown = false;
        let r = land_at(
            &mut slot.vcpu,
            &counter,
            inj.delivered_icount + 100,
            &Margins::default(),
            &mut |exit| {
                if matches!(exit, VcpuExit::Shutdown) {
                    saw_shutdown = true;
                    Err(BoundaryError::Exit("triple fault (expected)".into()))
                } else {
                    Err(BoundaryError::Exit(format!("unexpected: {exit:?}")))
                }
            },
        );
        assert!(r.is_err());
        assert!(saw_shutdown, "queued vector must deliver on next entry");
    }
}

#[cfg(test)]
mod sti_tests {
    use super::*;
    use crate::boot::load_and_enter;
    use crate::boundary::Margins;
    use crate::kvm::KvmSystem;
    use crate::run::install_kick_handler;
    use dh_detclock::counter::NEVER_FIRES_PERIOD;

    fn gettid() -> i32 {
        // SAFETY: argless syscall.
        #[allow(unsafe_code)]
        unsafe {
            libc::syscall(libc::SYS_gettid) as i32
        }
    }

    fn no_exits(exit: VcpuExit) -> Result<(), BoundaryError> {
        Err(BoundaryError::Exit(format!("unexpected exit: {exit:?}")))
    }

    /// THE §3.4 use case (the review's live-reproduced gap): request the
    /// injection while IF=0, defer through the NOP pad, and deliver at the
    /// first injectable boundary after STI + its shadow — without the
    /// deferral dying on a foreign exit.
    #[test]
    fn deferral_delivers_when_guest_opens_the_window_live() {
        let run = || {
            if !crate::kvm::kvm_usable() {
                eprintln!("skipping: /dev/kvm not usable");
                return None;
            }
            install_kick_handler().unwrap();
            let sys = KvmSystem::open().unwrap();
            let slot = sys.create_slot_vm(16 << 20).unwrap();
            load_and_enter(&slot, nanokernel::sti_window_elf(), b"").unwrap();
            let counter = InstRetired::open_for_current_thread().unwrap();
            counter
                .route_overflow_to_thread(gettid(), crate::run::kick_signal())
                .unwrap();
            counter.arm_period(NEVER_FIRES_PERIOD).unwrap();
            counter.reset().unwrap();
            counter.enable().unwrap();
            let mut slot = slot;

            // Land early, inside the IF=0 NOP pad (crt0 + a couple NOPs).
            let b = land_at(
                &mut slot.vcpu,
                &counter,
                3,
                &Margins::default(),
                &mut no_exits,
            )
            .unwrap();
            assert!(!injectable(&mut slot.vcpu).unwrap(), "still IF=0");

            let inj = inject_at_boundary(
                &mut slot.vcpu,
                &counter,
                0x31,
                &b,
                &Margins::default(),
                64,
                &mut no_exits,
            )
            .unwrap();
            assert!(
                inj.delivered_icount > inj.requested_icount,
                "window was closed at request time: delivery must be deferred"
            );

            // Delivery on the next entry: empty IDT => deterministic triple
            // fault, never another retired spin iteration.
            let mut saw_shutdown = false;
            let r = land_at(
                &mut slot.vcpu,
                &counter,
                inj.delivered_icount + 50,
                &Margins::default(),
                &mut |exit| {
                    if matches!(exit, VcpuExit::Shutdown) {
                        saw_shutdown = true;
                        Err(BoundaryError::Exit("triple fault (expected)".into()))
                    } else {
                        Err(BoundaryError::Exit(format!("unexpected: {exit:?}")))
                    }
                },
            );
            assert!(r.is_err());
            assert!(saw_shutdown);
            Some((inj.requested_icount, inj.delivered_icount))
        };
        let (Some(a), Some(b)) = (run(), run()) else {
            return;
        };
        assert_eq!(a, b, "deferred delivery boundary must replay identically");
    }

    #[test]
    fn exception_vectors_are_refused() {
        if !crate::kvm::kvm_usable() {
            eprintln!("skipping: /dev/kvm not usable");
            return;
        }
        let sys = KvmSystem::open().unwrap();
        let slot = sys.create_slot_vm(2 << 20).unwrap();
        assert!(matches!(
            queue_interrupt(&slot.vcpu, 14),
            Err(InjectError::BadVector { vector: 14 })
        ));
    }
}
