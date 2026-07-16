Findings:

1. P2: missing required capture-vs-no-capture equivalence coverage.
   Existing tests covered full/parentless checkpoint storage, dirty tracking preservation,
   entropy stability, and selected runtime fields, but did not compare a control leg after
   subsequent execution and normal TakeSnapshot.
