# Recommendation

Do not merge this checkpoint as-is.

Fix the DetChannel replay path first, then add a service-level VerifyReplay regression using the nanokernel capture fixture. Also pin down the `Run` failed-capture contract with a test; either make the state advancement explicit and observable or avoid returning an error after hiding the successful run boundary.

After fixes, rerun at least:

```bash
cargo test -p dh-worker capture -- --nocapture
cargo test -p dh-worker verify_replay -- --nocapture
cargo test -p dh-worker -- --nocapture
```
