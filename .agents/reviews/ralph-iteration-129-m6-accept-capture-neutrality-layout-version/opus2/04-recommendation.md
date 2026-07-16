# Recommendation

Request changes.

Do not treat `determinism-hypervisor-pee` as accepted until the service-level acceptance proves non-empty epoch-hash neutrality for capture versus no-capture runs of the same snapshot and inputs.

The child snapshot hash comparison and `layout_version` `FAILED_PRECONDITION` checks are useful and should stay. The acceptance needs a service-path epoch proof that cannot pass as `[] == []`, and the helper should not be the only non-empty epoch-hash evidence.
