# What The Bridge Provides For Verification

Same standing capability as prior requests
(`rom-bridge-getframebuffer-region-contract/06-deployed-verification.md`
shows the format): the deployed bridge at rombridge.birb.homes drives the
deployed worker over UDS `/run/dh/grpc.sock` with honest state reporting
and WARN-level gRPC error logging.

## Standing Offer

1. **Backstop, end-to-end (if item 4 lands as an implementation):** once a
   wall-clock backstop is deployed to the worker build, we will drive the
   idle-parked condition from the operator surface and confirm: the UI
   surfaces the distinguishable error instead of an infinite spinner, and
   the session recovers (stop → start works without a worker restart).
   If item 4 closes as confirm-no-hang instead, we'll retire our
   `timeout(1)` client stopgap and note that in the same handback.
   Results filed back into that request directory.
2. **Frame gate, human-visible half:** when the retuned `linux_m5` gate is
   green and refwork's regenerated READY snapshot is cut over, we run the
   `RestoreSnapshot → InjectInputs → Run → GetFramebuffer` loop from the
   browser and file the frame evidence — the same loop as Phase 3 exit
   gate 3.
3. **Deployment caveat we own:** bridge restarts orphan live slots
   (bridge bead `72o`); we coordinate the worker restart windows so your
   acceptance runs don't race our sessions.

## Contact / Tracking

- Bridge beads: `9xo` (open P0 — first-frame blocker; remaining chain is
  image rebuild → snapshot regen → operator cutover), `72o` (slot leases).
- The two request dirs this consolidates:
  `phase3-snapshot-restore-no-frame-under-no-tick-take-two/` (esp. `07-`/`08-`)
  and `nextsdkevent-run-wallclock-backstop/`.
- Joint dependency to watch: reference-workload `refwork-gp9` (image
  rebuild + READY snapshot regen) — filed against that repo separately.
