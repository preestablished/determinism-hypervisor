# 00-overview.md

Reviewed `ralph/iteration-157-add-linux-boot-to-ready-determinism-test` against `main` at checkpoint `748469d`.

The new gate mostly matches the bead's literal acceptance: it is ignored by default, fails loud when final env/KVM prerequisites are missing, performs two cold boots, stops only on detchannel Ready, compares `ready_icount`, parsed Ready identity fields, `machine_config_hash`, and `state_hash`, and scans the sealed input log for pre-Ready host input.

Primary concern: the test computes artifact hashes and populates the image cache, but the actual boot/block-device run still consumes the original env paths rather than the verified cache entries or exact reverified bytes.
