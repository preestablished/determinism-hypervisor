# Suggestions

### S1 — `DIRTY_RING_BYTES` is now dead; the inline `* 16` re-introduces the magic literal it was meant to name

Before this iteration, `cap.args[0] = DIRTY_RING_BYTES;` (kvm.rs) was the single consumer of `pub const DIRTY_RING_BYTES: u64 = DIRTY_RING_ENTRIES * 16;` (kvm.rs:21). The diff replaces that line with:

```rust
cap.args[0] = ring_entries * 16; // bytes of kvm_dirty_gfn
```

After this change, `DIRTY_RING_BYTES` has **zero consumers** in `crates/` (verified by grep — the only hit is its own definition). It's `pub`, so the dead-code lint stays quiet, but it's now an orphan const, and the meaningful name (`16 == sizeof(kvm_dirty_gfn)`) has been demoted to a bare literal + comment at the one site that matters.

Two clean options, pick one:
1. **Name the element size:** introduce `const DIRTY_GFN_BYTES: u64 = 16; // sizeof(kvm_dirty_gfn)` and write `cap.args[0] = ring_entries * DIRTY_GFN_BYTES;`. Then either delete `DIRTY_RING_BYTES` or redefine it as `DIRTY_RING_ENTRIES * DIRTY_GFN_BYTES` if anything external still wants it. This is more robust than `* 16` because `std::mem::size_of::<kvm_dirty_gfn>()` is the true source — consider `const { std::mem::size_of::<kvm_dirty_gfn>() as u64 }` with a `const _: () = assert!(... == 16)` pin so a kvm-bindings struct-size change can't silently mis-size the ring.
2. **Just delete `DIRTY_RING_BYTES`** if you accept the inline literal — but then the magic `16` lives un-named at the load-bearing site, which is the weaker choice.

I'd take option 1 with the `size_of` pin: the ring byte count feeding `cap.args[0]` is exactly where a wrong element size becomes a kernel-visible EINVAL or a mis-sized ring, so deriving it from the actual struct beats two independent `16`s.

### S2 — No asm-drift pin for `page_dirtier`; `PAGE_DIRTIER_START_GPA` is dead

Every other guest with asm `%define`s has a drift test in `tests/nanokernel/tests/elf_shape.rs` pinning the asm constants to the Rust twins: `landing_loop_asm_matches_rust_constants`, `rep_loop_asm_matches_rust_constants`, `timer_guest_table_gpa_matches`, `pad_echo_asm_matches_rust_constants`, `entropy_draw_asm_matches_rust_constants`. The pattern is established and consistent.

`page_dirtier` breaks the pattern. It defines, in asm (page_dirtier.asm:11–12):
```
%define START_GPA  0x200000
%define PAGES      3072
```
and in Rust (lib.rs:138–139):
```rust
pub const PAGE_DIRTIER_START_GPA: u64 = 0x20_0000;
pub const PAGE_DIRTIER_PAGES: u64 = 3072;
```
but **no test ties them**. Two consequences:
- **Drift risk:** if someone bumps `PAGES` in the asm to stress a larger ring but forgets `PAGE_DIRTIER_PAGES`, the test's `pages_shipped >= PAGE_DIRTIER_PAGES` floor (ring_chaos.rs:163) silently weakens — it could pass while shipping fewer pages than the guest actually wrote. That's precisely the kind of "floor stops protecting" rot the other drift tests exist to prevent.
- **Dead const:** `PAGE_DIRTIER_START_GPA` has **zero consumers** anywhere (verified by grep). It was presumably added for symmetry / for a drift test that never got written. Either wire it into a drift test or delete it.

Suggested addition (mirrors `timer_guest_table_gpa_matches`):
```rust
#[test]
fn page_dirtier_asm_matches_rust_constants() {
    let asm = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/asm/page_dirtier.asm")).unwrap();
    let define = |name: &str| -> u64 { /* same %define parser as pad_echo */ };
    assert_eq!(define("START_GPA"), PAGE_DIRTIER_START_GPA);
    assert_eq!(define("PAGES"), PAGE_DIRTIER_PAGES);
}
```
This both kills the dead const and makes the test's `pages_shipped` floor trustworthy. Low effort, restores consistency with the twelve sibling guests.

### S3 (minor) — `map_sized` parameter doc could state the power-of-two precondition it inherits

`create_slot_vm_with_ring` rejects non-power-of-two ring sizes (kvm.rs:138), but `map_sized` accepts any `entries` and will happily `% self.entries` against a non-power-of-two — only the masking-arithmetic comment on `entries` (dirty.rs:46) implies the constraint. Not a bug (callers get the size from a slot that was validated at create), but if I1 isn't taken and `map_sized` stays the public size-taking path, a one-line "`entries` must equal the slot's validated ring size" on the param is cheap insurance. Subsumed by I1 if that's adopted.
