# Critical And Important Issues

## Critical

- None.

## Important

### Important: Linux entry smoke no longer fails on immediate fatal KVM exits

File: `tests/determinism/tests/linux_boot_trace.rs:88`

Problem: Before this branch, `linux_entry_smoke` explicitly failed if the first `KVM_RUN` returned `Shutdown`, `InternalError`, or `FailEntry`, preserving the smoke test's core contract that Linux reached a serviceable exit after entry. The new trace path records those exits as `terminal_reason` and still lets the test pass after writing the artifact. A triple fault or fail-entry regression in the Linux entry path would now be reported as a successful ignored test.

Suggested fix snippet:

```rust
fn fatal_before_serviceable_exit(trace: &LinuxBootTrace) -> bool {
    trace.total_exits == 1
        && trace.terminal_reason.as_deref().is_some_and(|reason| {
            reason == "shutdown"
                || reason == "internal_error"
                || reason.starts_with("fail_entry(")
        })
}

// After trace_linux_boot(...) returns:
assert!(
    !fatal_before_serviceable_exit(&trace),
    "Linux entry failed before the first serviceable KVM exit: {}",
    trace.terminal_reason.as_deref().unwrap_or("unknown")
);
```

### Important: Trace zero-fills detchannel and serial `IN` exits before classification

File: `tests/determinism/tests/linux_boot_trace.rs:313`

Problem: `prepare_exit_for_trace` fills every `VcpuExit::IoIn` buffer with zero before `classify_exit` can identify whether it is `DetcallIn` or `SerialIn`. The classifier documents that these `IN` exits must be answered by the caller before re-entry; otherwise the guest sees the wrong ABI value. If this trace reaches `IN 0xD370` or a serial input, it will continue with a fabricated zero response, which can distort the later trace and hide the real detchannel readiness boundary.

Suggested fix snippet:

```rust
fn prepare_exit_for_trace(trace: &mut LinuxBootTrace, exit: &mut VcpuExit<'_>) {
    match exit {
        VcpuExit::IoIn(port, data)
            if !is_detchannel_port(*port) && !is_serial_port(*port) =>
        {
            data.fill(0);
        }
        VcpuExit::MmioRead(gpa, data) => {
            trace.observe_mmio(*gpa);
            data.fill(0);
        }
        // unchanged cases...
        _ => {}
    }
}

fn is_detchannel_port(port: u16) -> bool {
    let end = dh_vmm::kvm::PIO_DETCALL_BASE + dh_vmm::kvm::PIO_DETCALL_LEN;
    (dh_vmm::kvm::PIO_DETCALL_BASE..end).contains(&port)
}

fn is_serial_port(port: u16) -> bool {
    let end = dh_vmm::kvm::PIO_SERIAL_BASE + dh_vmm::kvm::PIO_SERIAL_LEN;
    (dh_vmm::kvm::PIO_SERIAL_BASE..end).contains(&port)
}
```

Then stop the trace when the first detchannel event is observed, or wire the real detchannel/serial handlers before re-entering KVM:

```rust
let reached_detchannel = observe_classified_event(&mut trace, &event);
if reached_detchannel {
    trace.terminal_reason = Some("first_detchannel_reached".to_string());
    return trace;
}
```
