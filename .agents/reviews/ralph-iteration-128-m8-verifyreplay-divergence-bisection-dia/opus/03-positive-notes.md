## Positive Notes

- `ReplayError::Divergence` now carries coarse `rip_actual`, which is a useful starting point for real boundary diagnostics.

- `RegDiff` is a clear serializable shape and its postcard round trip is covered by tests.

- The RPC test exercises both explicit `bisect_on_divergence` modes through the service path.
