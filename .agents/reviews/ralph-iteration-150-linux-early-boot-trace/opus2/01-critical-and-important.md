# Critical And Important

## Critical

None.

## Important

### Important: Trace loop zero-fills detchannel and serial PIO input buffers before classification

File: `tests/determinism/tests/linux_boot_trace.rs:313`

Problem: `prepare_exit_for_trace` fills every `VcpuExit::IoIn` buffer with zero before `classify_exit` sees it. That is safe for ignored/unmapped PIO, but detchannel and serial input ports have an explicit raw-exit fill contract: callers must either answer those buffers with the real deterministic device model or stop before re-entering KVM. Filling them with zero makes the trace loop change guest-visible behavior while still reporting a normal classified `DetcallIn`/`SerialIn`. That can mask stale-buffer regressions, make the first detchannel status misleading, or steer Linux down a path the real run loop would not take.

Suggested fix snippet:

```rust
fn prepare_exit_for_trace(trace: &mut LinuxBootTrace, exit: &mut VcpuExit<'_>) {
    match exit {
        VcpuExit::IoIn(port, data) if is_detchannel_pio(*port) => {
            trace.observe_detchannel("in", *port, data.len());
            // Either answer with the real deterministic detchannel model here,
            // or make this a terminal trace reason before re-entering KVM.
        }
        VcpuExit::IoIn(port, data) if is_serial_pio(*port) => {
            // Serve through DebugSerial::pio_read, or terminate the trace.
        }
        VcpuExit::IoIn(_, data) => data.fill(0),
        VcpuExit::MmioRead(gpa, data) => {
            trace.observe_mmio(*gpa);
            data.fill(0);
        }
        VcpuExit::MmioWrite(gpa, _) => trace.observe_mmio(*gpa),
        VcpuExit::IrqWindowOpen => trace.irq_window_open_count += 1,
        VcpuExit::Intr => trace.intr_count += 1,
        VcpuExit::IoapicEoi(vector) => {
            trace.ioapic_eoi_vectors.insert(*vector);
        }
        _ => {}
    }
}

fn is_detchannel_pio(port: u16) -> bool {
    let end = dh_vmm::kvm::PIO_DETCALL_BASE + dh_vmm::kvm::PIO_DETCALL_LEN;
    (dh_vmm::kvm::PIO_DETCALL_BASE..end).contains(&port)
}

fn is_serial_pio(port: u16) -> bool {
    let end = dh_vmm::kvm::PIO_SERIAL_BASE + dh_vmm::kvm::PIO_SERIAL_LEN;
    (dh_vmm::kvm::PIO_SERIAL_BASE..end).contains(&port)
}
```

### Important: Full trace mode implicitly requires perf/PMU availability

File: `tests/determinism/tests/linux_boot_trace.rs:440`

Problem: `trace_icount_limit` returns `Some(DEFAULT_TRACE_ICOUNT_LIMIT)` whenever `DH_M9_TRACE_BOOT=1`, so trace mode always enters `trace_linux_boot_with_icount`. That path has hard `expect` calls for `InstRetired::open_for_current_thread`, signal routing, reset, and enable at `tests/determinism/tests/linux_boot_trace.rs:245`. On a host with usable KVM and supplied Linux artifacts but without the pinned guest instruction counter permissions/configuration, the trace fails before producing the requested artifact. The exit limit already bounds the diagnostic run, so tying artifact production to PMU availability makes this test host-fragile.

Suggested fix snippet:

```rust
fn trace_icount_limit() -> Option<u64> {
    std::env::var(TRACE_ICOUNT_LIMIT_ENV).ok().map(|raw| {
        raw.parse::<u64>()
            .unwrap_or_else(|_| panic!("{TRACE_ICOUNT_LIMIT_ENV} must be a u64, got {raw:?}"))
    })
}
```

If the default instruction-count cap is required for trace runs, prefer returning a trace artifact with an explicit setup failure instead of panicking:

```rust
let counter = match InstRetired::open_for_current_thread() {
    Ok(counter) => counter,
    Err(e) => {
        trace.terminal_reason = Some(format!("icount_unavailable: {e:?}"));
        return trace;
    }
};
```
