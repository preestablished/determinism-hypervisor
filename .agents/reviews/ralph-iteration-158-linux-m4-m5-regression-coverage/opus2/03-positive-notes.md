# 03-positive-notes.md

The artifact helper only skips when `DH_M9_ALLOW_SKIP=1`; final commands with `DH_M9_ALLOW_SKIP=0` fail loud on missing env vars or dirty-ring KVM.

The M4 Linux path destroys the original READY lease before restore, uses the real worker service/snapstore, and checks state hash, machine config hash, frame counter, and stored EVTC/BLKO immutability.

The m5_net Linux test does run `VerifyReplay` through the worker service and checks final READY state hash plus source BLKO immutability.

Slot counts look intentional: M4 uses three slots for restored parent plus two fork children; m5_net keeps one live READY lease while reserving the second slot for `VerifyReplay`.
