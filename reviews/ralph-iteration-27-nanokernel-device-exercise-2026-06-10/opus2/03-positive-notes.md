# Positive Notes

Credit where due. I checked these by disassembly and by executing the real
guest-sdk attach/drain code, not by eyeballing the source.

### P1 — Every device register map and detcall port matches the host constants exactly

Cross-checked the guest's `%define`s against `crates/dh-devices/src/{clock,
entropy,pad,blk}.rs` and `detguest-wire/src/ports.rs`:

- clock: VNS `0x08`, ICOUNT `0x10` ✓
- entropy: BUF_GPA `0x08`, LEN `0x10`, DOORBELL `0x14`, STATUS `0x18`,
  `STATUS_OK = 1` ✓
- pad: PAD0 `0x08` ✓
- blk: SECTOR `0x08`, BUF_GPA `0x10`, COUNT `0x18`, CMD `0x1C`, STATUS `0x20`,
  `CMD_READ = 1`, `CMD_WRITE = 2`, `STATUS_OK = 0` ✓
- detcall ports: INIT_LO `0xD374`, INIT_HI `0xD378`, INIT_GO `0xD37C`,
  DOORBELL `0xD380` ✓

Notably, the program correctly distinguishes the two opposite "OK" conventions
— `BLK_STATUS_OK = 0` vs `ENT_STATUS_OK = 1` — and uses each in the right
`cmp`/`jne`. That's the exact mixup the prompt warned about, and it isn't here.

### P2 — IN/OUT widths and `dx` discipline are correct

Disassembly confirms all detcall OUTs are 32-bit (`ef out %eax,(%dx)`) and the
status/doorbell reads are 32-bit (`ed in (%dx),%eax`). `dx` is loaded with the
port via `66 ba .. ..` (16-bit `mov dx, imm16`) before each OUT and is *not*
clobbered by the OUT, so the immediately-following `in (%dx),%eax` reads the
same port — correct for the INIT_GO and DOORBELL read-after-write pairs. No
16-bit/32-bit width confusion anywhere.

### P3 — Channel record framing is byte-correct

The Beacon record (`len 24`, `kind 5`, `flags 0`, `seq 0`, `vnanos`,
`beacon_id 0xB33F`, pad) matches `detguest-wire`'s record layout
(`record.rs`: 2B len | 1B kind | 1B flags | 4B seq | 8B vnanos | payload) and
the Beacon decoder's `payload.len() >= 8` check (`events.rs:562`). My scratch
run drained it to exactly `Beacon { beacon_id: 0xB33F }`, ring W, seq 0 — once
the W *size* is fixed. `seq 0` is correct (per-ring seq starts at 0,
`record.rs:23`), and the producer index publish (`prod = 24`) makes exactly
this one record available with `avail = 24 - 0 = 24`, which the drain loop
consumes fully and then stops (`avail` drops to 0 < `PAD_MIN_LEN`).

### P4 — INIT_GO commit value and `mem_size` arithmetic are right

`CHANNEL_PAGES = 512` is committed to INIT_GO (`mov $0x200,%eax`), matching the
host's `CHANNEL_SIZE_PAGES = 0x200000/4096 = 512` (`detchannel.rs:305` rejects
any other size as status 2). The `mem_size` gate folds
`CHANNEL_GPA + 0x200000 = 0x600000` at assemble time (`objdump`:
`cmp $0x600000,%rax`) — nasm constant-folded it correctly, no operator-
precedence surprise.

### P5 — Doorbell mask is correct and the host accepts it

`mov eax, 2` for the doorbell (`DOORBELL_RING_W = 1 << 1`), matching
`detchannel.rs:52`. The host drains both guest→host rings on any nonzero mask
and returns a defined `0` for `IN 0xD380` (`detchannel.rs:273`), which the
guest checks. The empty-mask metric path is avoided.

### P6 — Single-vCPU memory ordering is handled honestly

The comment "publish producer index (single vCPU: the OUT below orders it)" is
correct: there's no second consumer racing the guest, and the subsequent
`OUT 0xD380` is a serializing VM exit, so the producer-index store is visible
to the host before it drains. No bogus fence ceremony, no missing one.

### P7 — Clean failure model and good documentation

The lowercase-on-failure / park-immediately convention is simple and makes the
serial log self-describing ("CEPBDX" = success, any lowercase = the failing
stage). `putc` is minimal and clobbers only DX. The module header documents
the clean-room provenance of every constant and the channel layout. crt0's
HLT-park terminal stop is respected (`prog_main` returns, never falls through).
The only thing the documentation got wrong is the W ring size — and that came
from a contradictory upstream doc, not from sloppiness here.
