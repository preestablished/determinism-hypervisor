# 02-suggestions.md

**Suggestion - `tests/determinism/tests/linux_ready.rs:73` and `tests/determinism/tests/linux_ready.rs:134` - boot and pv-blk bytes are read from source artifact paths after hashing/cache population - risk: a concurrent artifact mutation could make the actual boot/input bytes differ from the hashes embedded in `MachineConfig`, while the two-run comparison still passes. Recommended fix: either read/open the populated cache entries by content hash, or hash the loaded `bzimage`, `initramfs`, and opened `game_image` bytes immediately before use and assert they still match `hashes`.**

**Suggestion - `tests/determinism/tests/linux_ready.rs:241` - the pre-Ready host-input predicate is important enough to unit test directly - recommended fix: add small DHILOG writer-based tests for rejecting `PadSet`, `NetRx`, non-detchannel `DevEvent`, and detchannel ring C/I pushes before Ready.**
