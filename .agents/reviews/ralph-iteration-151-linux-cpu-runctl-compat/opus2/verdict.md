REQUEST_CHANGES

The branch adds useful Linux CPU compatibility classification, but it does not enforce that the state-hashed CPUID table matches the CPUID table actually installed on the vCPU, and the live trace test does not fail when unclassified exits are present.
