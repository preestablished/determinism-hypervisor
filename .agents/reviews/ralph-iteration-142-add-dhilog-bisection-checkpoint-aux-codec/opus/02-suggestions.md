# 02-suggestions.md
- `crates/dh-inputlog/tests/golden.rs:49`: The `build_kitchen_sink` doc still says it covers “every writer-emittable kind,” but `BISECTION_CHECKPOINT` is now writer-emittable and intentionally excluded from the v1.0 fixture. Suggested wording: “The original v1.0 kitchen-sink build...”.

- `crates/dh-inputlog/src/reader.rs:671`: Consider whether unsupported nested checkpoint versions should make sealed replay fail, or whether they should remain skippable as AUX evidence. If future v1.x writers reuse kind `0x46` with a higher nested format version, this reader currently rejects the whole log even though AUX is otherwise designed to be ignorable by minimal replay.
