Good patterns to preserve:

- The change avoids renumbering proto fields and keeps ForkRequest source-compatible.
- The mapper converts wire bytes into `Option<[u8; 32]>`, which makes the future service path pass semantic seed intent rather than raw vectors.
- The engine keeps snapshot-equivalent fork behavior as the default, preserving existing M4 transparency tests.
- The tests include both request-shape validation and KVM-backed engine behavior.

