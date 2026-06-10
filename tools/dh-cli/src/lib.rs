#![forbid(unsafe_code)]

//! dh-cli library surface: the boot path is a lib module so the
//! integration tests can drive it in-process (the M0 acceptance boots
//! hello.elf live wherever /dev/kvm is usable).

pub mod boot;
