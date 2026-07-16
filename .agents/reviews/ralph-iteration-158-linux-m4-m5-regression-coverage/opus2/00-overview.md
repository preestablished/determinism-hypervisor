# 00-overview.md

Reviewed `ralph/iteration-158-linux-m4-m5-regression-coverage` against `main` at checkpoint `76c5f74`.

The shared M9 helper has solid skip semantics: with `DH_M9_ALLOW_SKIP=0`, missing artifacts/KVM fail instead of silently skipping. The M4 Linux test is a reasonable READY snapshot/restore/fork smoke path.

I found two acceptance-significant gaps: the Linux frame scheduling gate can pass after deterministic halt without observing any frame, and the pv-blk fallback tests `PvBlk` in isolation while `VerifyReplay` only covers the separate READY boot log.
