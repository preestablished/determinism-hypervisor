#![forbid(unsafe_code)]
#![allow(clippy::result_large_err)]

//! dh-cli library surface: the boot path is a lib module so the
//! integration tests can drive it in-process (the M0 acceptance boots
//! hello.elf live wherever /dev/kvm is usable).
//!
//! Everything here drives KVM/VMX and is x86_64-only; the gates keep
//! the crate buildable on other arches (bead v5w) so CI's plain
//! `--workspace` legs cover it without an exclude list.

#[cfg(target_arch = "x86_64")]
pub mod boot;
#[cfg(target_arch = "x86_64")]
pub mod cli;
#[cfg(target_arch = "x86_64")]
pub mod cpuid;
#[cfg(target_arch = "x86_64")]
pub mod gate;
#[cfg(target_arch = "x86_64")]
pub mod linux;
#[cfg(target_arch = "x86_64")]
pub mod ops;
#[cfg(target_arch = "x86_64")]
pub mod run;
#[cfg(target_arch = "x86_64")]
pub mod skid;
