# 00-overview.md

Reviewed branch `ralph/iteration-157-add-linux-boot-to-ready-determinism-test` against `main`.

The new ignored `linux_ready` gate covers the bead acceptance shape: it requires M9 artifacts unless `DH_M9_ALLOW_SKIP=1`, performs two cold boots, waits specifically for detchannel Ready via `NextSdkEvent`, compares Ready identity fields plus machine/state hashes, and rejects pre-Ready host input from the sealed DHILOG.

I found one important issue in the custom detchannel exit path: it does not surface `LogWriter` faults after detchannel PIO handling.
