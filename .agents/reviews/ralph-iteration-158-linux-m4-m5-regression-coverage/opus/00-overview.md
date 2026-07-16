# 00-overview.md

Reviewed `ralph/iteration-158-linux-m4-m5-regression-coverage` against `main` at checkpoint `76c5f74`.

The Linux M4 restore/fork test is a reasonable READY-boundary regression for today's M9 fixture. The Linux M5 frame test is intentionally narrow: it validates deterministic behavior across identical READY restores and accepts the current fixture limitation where Linux halts before frame marks.

I found one important acceptance-coverage issue in the Linux `m5_net_loopback` replacement: the pv-blk overlay exercise is disconnected from the replay/DHILOG/state-hash verification, so the acceptance can pass without proving replay of the pv-blk replacement workload.
