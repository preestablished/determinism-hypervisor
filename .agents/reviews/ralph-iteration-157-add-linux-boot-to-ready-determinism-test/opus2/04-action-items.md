# 04-action-items.md

1. Prefer verified cache blobs, or reverify the exact bytes/descriptors used for boot and pv-blk, before accepting the artifact identity.

2. Decide whether this bead needs the stronger READY semantic from the M9 planning docs; if yes, add Hello/region ordering assertions.

3. Optionally reject duplicate Ready events observed before the stop boundary.
