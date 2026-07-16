# Action items — ralph iteration 46 (ws4 CoW fixture)

### Critical

_None._

### Important

_None._

### Suggestions

- [ ] [crates/dh-vmm/tests/blk_fixture.rs phase 3] Add one read request that
  crosses a clean→dirty cluster boundary so the device's per-cluster chunk loop
  is exercised across a clean/dirty mix *by this test* (currently no BATCH=64
  request spans a boundary, since reads are 64-aligned and a cluster is 128
  sectors; the only boundary-crossing unit test has both clusters dirty). I
  verified the device handles the mix correctly via a throwaway test, so this is
  coverage, not a bug. Concrete patch: after phase 2, read sectors 96..160
  (cluster 0 clean tail + cluster 1 dirty head) and assert each sector against
  `image::expected_sector_after_writes(sec)`. See 02-suggestions §S1.

- [ ] [crates/dh-vmm/tests/blk_fixture.rs:32 / tests/nanokernel/src/image.rs:170]
  Optional: make the temp-file collision-safety self-evident. Naming uses only
  `process::id()` + tag; it is collision-free because the file-creating tests
  live in separate test binaries (distinct pids) and `blk_fixture`'s two file
  tests use distinct tags, and `File::create`/`fs::write` truncate. Add a
  one-line comment stating that invariant (or append the tag to the nanokernel
  test path too). Lowest priority. See 02-suggestions §S2.
