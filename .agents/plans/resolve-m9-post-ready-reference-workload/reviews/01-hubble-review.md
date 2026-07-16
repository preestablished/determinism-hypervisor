# Subagent Review 1 - Hubble

Verdict: `REQUEST_CHANGES`

## Critical Issues

- False-positive worker gates: `04-test-and-acceptance-gates.md` and
  `06-bead-handoff.md` used `DH_M9_GUEST=linux` and/or a `linux` test filter
  for `m4_transparency`, `m5_frame_scheduling`, `m5_net_loopback`, and
  `m5_record_replay`. Those selectors are not wired today; a Linux filter can
  select zero tests and still exit successfully. The plan must require exact
  Linux test names or an explicit nonzero `--list` guard before these commands
  can count as evidence.
- False-positive M7 Linux acceptance: the plan relied on
  `DH_M7_ACCEPT_GUEST=linux`, but current `m7_fork_verify` does not read that
  env var and still boots `nanokernel::pad_echo_elf()`. This could close Linux
  M7 on nanokernel evidence.

## Important Issues

- The plan omitted `4s9.22` and treated `4s9.24` only as downstream, even
  though both are blocked and `4s9.24` directly requires deterministic
  post-READY budget evidence.
- Fixture-builder ownership was vague. The plan needs expected paths, discovery
  commands, or a bead/external issue to own the builder.
- `06-bead-handoff.md` should call out the known `linux_worker_api`
  VerifyReplay divergence separately from the manifest failure.

## Suggestions

- Add a universal acceptance guard: every Linux-filtered command must first
  prove at least one Linux test is selected, and the test itself must fail if
  the Linux guest path/env selector is unsupported.
- Add close criteria for `4s9.24` and artifact-backed `4s9.22`, or explicitly
  explain why they are outside this plan.
- Keep the anti-false-positive language in `05-risks-and-debugging.md`; align
  command sections to that standard.
