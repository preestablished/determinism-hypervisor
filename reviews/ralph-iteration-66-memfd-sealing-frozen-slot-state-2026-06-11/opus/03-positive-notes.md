# Positive Notes

## P-1. Seal choice is correct and the reasoning is preserved in code

`freeze_ram` (`kvm.rs`) applies exactly the right set:
`F_SEAL_FUTURE_WRITE | F_SEAL_SHRINK | F_SEAL_GROW`.

- **`F_SEAL_FUTURE_WRITE`** matches ARCH §8.4 verbatim — it blocks *new* writable
  mappings (CoW children and any other process get read-only views of the baseline)
  without touching the parent's existing KVM mapping.
- **`F_SEAL_SHRINK | F_SEAL_GROW`** are a well-justified ride-along: a truncate on a
  frozen parent would rip pages out from under every CoW child (a real corruption
  vector), and resizing frozen RAM is never legitimate. The live test proves SHRINK
  bites (`file.set_len(1024).is_err()`).
- **`F_SEAL_WRITE` deliberately omitted** with an accurate explanation: the kernel
  refuses it (`EBUSY`) while the parent's live writable KVM mapping exists, which is
  precisely *why* the software `Frozen` guard must exist. The diff does not paper over
  this — it names it as the load-bearing reason for the two-halves design.
- **`F_SEAL_SEAL` omitted for idempotence** — re-applying the same seals is a kernel
  no-op only if the fd isn't itself sealed against further seals. The test pins this
  invariant explicitly (`assert_eq!(seals & F_SEAL_SEAL, 0); // idempotence depends on this`)
  and then proves it by re-calling `freeze_ram`. Excellent: the comment's claim is
  executable.

On the same-process threat model: omitting `F_SEAL_SEAL` means a hostile in-process
actor holding the fd could add `F_SEAL_WRITE` later — but this is a same-process,
same-trust-domain VMM; an actor with the fd already has the parent's address space and
can corrupt RAM directly regardless of seals. The trade is sound; sealing-the-seal would
buy no real defense and would cost idempotence (which the fork path needs).

## P-2. Transition relation is exactly the spec relation, and the test proves *exactly*

`can_transition` encodes the §2.2/§8.4 relation as a single `matches!`, and
`transition_matrix_is_exactly_the_spec_relation` checks the **full 4×4 grid** against an
independent `allowed` list — so it catches both missing-allowed and extra-allowed bugs,
not just the happy path. `no_self_transitions` and `running_cannot_be_destroyed_or_frozen_directly`
pin the two most important rejections (`Running→Empty` must pause first; `Running→Frozen`
because fork requires a Paused parent). `fork_lifecycle_walk` exercises the real
`Empty→Paused→Frozen→Paused→Frozen→Empty` cycle including the "freeing the last child
unfreezes the parent" (§2.2) and re-fork edges. This is the right way to test a state
machine — relation-as-data, checked exhaustively.

## P-3. Loud-by-design error type with the caller name

`SlotStateError::FrozenWriteDenied { api: &'static str }` (and `EmptyWriteDenied`,
`InvalidTransition`) carry enough context to make a denial diagnosable at the failure
site — the `api` field names the offending call. The doc-comment on `SlotStateError`
correctly frames this as "the only thing standing between a stray engine call and
corrupting every CoW child's shared baseline," which is the accurate R9 stakes. `Empty`
gets its own `EmptyWriteDenied` rather than being lumped into the frozen case — a clean
distinction (nothing to write vs. must-not write).

## P-4. The live test genuinely proves all four R9 properties

`freeze_ram_seals_future_writes_but_not_the_live_mapping` is a real end-to-end proof,
not a mock:
1. seal lands (FUTURE_WRITE/SHRINK/GROW set, SEAL clear) post-`freeze_ram`;
2. a new `PROT_WRITE|MAP_SHARED` mmap returns `MAP_FAILED` with `EPERM`;
3. a new `PROT_READ` mapping succeeds (children can read the baseline);
4. the parent's existing mapping stays writable (`write_slice` succeeds post-seal — the
   exact behavior that necessitates the software guard);
plus idempotence (re-freeze) and shrink-denial. Pre-freeze it asserts the FUTURE_WRITE
bit is *absent*, so the test can't pass on a memfd that was somehow already sealed. It
correctly guards on `kvm_available()` and skips cleanly when `/dev/kvm` is absent.

## P-5. Clean resource hygiene in the test

The RO mapping is `munmap`'d after the assertion; the `MAP_FAILED` (write) path correctly
does *not* munmap (there's nothing to unmap). No fd leaks — the memfd lives in the
region's `FileOffset` and is owned by `guest_mem`. Tight.

## P-6. Unsafe usage is minimal, localized, and honest

Five `#[allow(unsafe_code)]` sites, each wrapping a single libc FFI call (`fcntl` ×2 in
`freeze_ram`/`ram_seals`; `mmap` ×2 + `munmap` ×1 in the test) with the result checked
immediately on the next line. The crate is `#![deny(unsafe_code)]`-style (per-site
allows), so each unsafe block is a deliberate, reviewable exception rather than a blanket
opt-out. The fcntl error handling is correct: `freeze_ram` checks `rc != 0`, `ram_seals`
checks `seals < 0`, and both attach `std::io::Error::last_os_error()` — read immediately
after the syscall, before any other libc call can clobber `errno`.

## P-7. Spec citations are precise and verifiable

Comments cite ARCH §8.4, API §2.2, and risk R9 — and I verified each against the source
docs. The §8.4 text ("`F_SEAL_WRITE` is unusable here: it fails `EBUSY` while any writable
shared mapping exists … the guard … is the `Frozen` slot-state machine") maps one-to-one
onto the code's two-halves split. The citations are load-bearing and accurate, not
decorative.
