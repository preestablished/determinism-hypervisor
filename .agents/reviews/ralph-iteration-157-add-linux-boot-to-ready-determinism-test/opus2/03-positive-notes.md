# 03-positive-notes.md

The final-gate skip behavior is correct: `DH_M9_ALLOW_SKIP=0` or unset does not skip missing artifacts/KVM.

The stop condition is tied to detchannel Ready, not serial text, and wrong stream/wrong payload shape fail.

The pre-Ready input scan rejects `PAD_SET`, `NET_RX`, non-detchannel device events, and detchannel ring C/I host pushes through the sealed DHILOG.

The test avoids a `dh-worker` dependency while still exercising the direct VMM boot path.
