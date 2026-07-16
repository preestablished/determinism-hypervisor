# 03-positive-notes.md

The shared M9 helper correctly fails closed when `DH_M9_ALLOW_SKIP=0`: missing artifacts or unusable KVM return errors instead of silently skipping.

The Linux frame test records the current fixture limit in output and compares two independent READY restores, including reason, delta icount, frames elapsed, and state hash.

The M4 Linux test checks restored and forked READY state hashes, machine config hash, frame counter, and verifies stored EVTC/BLKO sections were not mutated by restore/fork operations.

The new `input_log_payload` helper decodes the snapstore input-log container instead of ad hoc byte handling.
