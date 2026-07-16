# Critical & Important findings

**None.**

I probed every implicit assumption the review brief called out and could not
turn any of them into a defect. Recording the negative results, because the
*absence* of these bugs is the substantive finding for this guest:

## Probed and cleared

### Spin loop register usage (`loop` + `mov eax,[RX_LEN]`) — CORRECT
`tests/nanokernel/asm/net_loopback.asm:96-103`. The poll reads into `EAX`
only; `RCX` is the loop counter set to `SPIN_BUDGET` and is never touched by
the MMIO read. `loop .spin` decrements `RCX` and branches — correct. In long
mode `loop` uses the full 64-bit `RCX`, and `mov rcx, SPIN_BUDGET` loads it
correctly. No clobber, no off-by-one that matters (65536 iterations is a
bound, not an exact count).

### "Harness lands NET_RX between polls → guest misses it" — IMPOSSIBLE
This was the sharpest concern, and the device contract closes it. `RX_LEN` is
**sticky**: `apply_net_rx` sets `self.rx_len = len` (`net.rs:159`) and the only
thing that clears it is the guest's own `REG_RX_LEN` write (`net.rs:196`). So a
delivery landing between two polls is observed by the *next* poll — there is no
edge the guest can miss. The bounded budget therefore only fires when the
harness genuinely never delivers, which is exactly the documented `'r'` meaning.

### `RX_LEN == FRAME_LEN` strictness vs a "different length" delivery — SAFE
`net.rs:154` rejects `len == 0`, `len > MAX_FRAME`, and `len > rx_cap` with
`FrameTooBig` (no partial/clamped delivery — it copies the full frame or errors
out). For the loopback acceptance the re-landed frame is the same frame the
guest TXed, so `RX_LEN == FRAME_LEN` always holds on success. A wrong length
would be a real harness/device bug and `'r'` is the correct loud signal.

### `mem_size` check precedes the frame fill at TX_GPA — CONFIRMED
`net_loopback.asm:60-66` runs the BootInfo + `mem_size >= RX_GPA +
RX_CAP_BYTES` check *before* the `.fill` loop at `TX_GPA` (`:69-75`). Since
`TX_GPA (0x200000) < RX_GPA (0x210000)` and the check covers `RX_GPA +
RX_CAP_BYTES`, the TX fill region is transitively covered. No out-of-RAM write.

### `inc al` vs `(u32 + i) as u8` — EQUIVALENT at FRAME_LEN=64 (and any ≤256)
Both are mod-256 sequences from `0x5A`. At len 64 neither wraps
(`0x5A + 63 = 0x99`), and even past the wrap point (e.g. `0x5A + 196 = 286`)
both wrap identically because `inc al` is 8-bit and `as u8` truncates. The
drift test pins `frame[63] == BYTE_BASE.wrapping_add(63)`. No mismatch at any
`FRAME_LEN <= 256`. (See suggestion 02#1 for the >256 edge — not reachable
today.)

### RX_VECTOR stays 0 — CONFIRMED on both sides
The asm never writes `REG_RX_VECTOR` (only comments reference it). `PvNet::new`
initializes `rx_vector: 0` (`net.rs:96`), and `apply_net_rx` returns
`None` (no injection) when `rx_vector == 0` (`net.rs:160`). Polling path is
internally consistent; leaving `REG_RX_VECTOR` out of the drift pin is fine
because the guest neither reads nor writes it.

### `STATUS_OK` pinned as a value, not an offset — CORRECT
`elf_shape.rs:480-483` compares `define("STATUS_OK")` (== 1) against
`dh_devices::net::STATUS_OK` via `u32::try_from`, distinct from the offset
`%define`s. The asm uses it only in a `cmp eax, STATUS_OK` value compare
(`:90`), never as an MMIO offset. Naming is a value, used as a value. Good.

### Single-shot (no re-arm) adequacy for czq record+replay — ADEQUATE
The guest does one TX + one RX and parks. That is exactly one `NET_TX` AUX
record and one `NET_RX` canonical record — the minimal recordable/replayable
unit. Bit-identical replay needs no re-arm. Correct by design.
