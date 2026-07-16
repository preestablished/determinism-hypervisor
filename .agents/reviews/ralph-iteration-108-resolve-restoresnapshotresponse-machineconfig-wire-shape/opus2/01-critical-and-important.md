No critical or important issues found.

Reviewer notes:
- The proto/API additions use new field numbers and do not renumber existing MachineConfig fields.
- resync_slack remains intentionally non-wire and defaults during inbound mapping.
- Canonical equality plus targeted field assertions cover the meaningful round-trip contract.

