# Action Items

1. Add a fixture-observed decision proof and compare it exactly to the decoded DHILOG `PORT_INJECT` answer sequence.

2. Isolate post-READY `InjectQuery` evidence by draining the READY backlog before the GS-7 run or filtering events by the READY snapshot icount boundary.

3. Count nontrivial decisions using `FaultDecision::unpack`, and reject noncanonical raw answer values.
