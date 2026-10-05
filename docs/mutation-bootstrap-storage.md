# Mutation driver bootstrap storage

The driver is compiled with the locked workspace in the ignored source-local
`.cache/mutation/driver-target` directory. Its bootstrap artifacts no longer
compete with the scanner for the same 4 GiB `/tmp` mount. This is a thin Make
environment change, not a trusted compiler-cache or executed-binary attestation.
The coding UID controls this cache and Docker; reuse remains non-authoritative.

The selected immutable Snapshot, fresh **0700** scanner target and raw output,
strict namespace/identity checks, fixed Git guards, locked/offline scanner children,
source/export/private-Git seals and held artifact descriptors are unchanged.
Fresh scanner storage stays external to source, snapshot, Git/common-Git, Cargo
home, default/current target and source caches. It must never reuse the driver
cache. A real retained-Snapshot subprocess test verifies disjoint fresh roots,
explicit driver-target exclusion and unchanged cache sentinel bytes.

Previous actual campaign failed with log-write **No space left on device** after
its full unmutated baseline passed and 34 mutants were generated. That is not
34 evaluated mutants or a qualified campaign. Failure evidence remains retained.
The capacity layout motivates this repair; it is not authenticated root-cause,
compiler provenance, independent worker authority or dedicated hardware proof.

No increase to the 4 GiB mount, 120-second mutant / 4500-second command budgets,
>=30 evaluated floor, zero missed/timeouts, original test baseline, file/filter
selection or 85% overall / 90% critical coverage. Retry the actual unchanged
campaign on the new exact committed source; incomplete artifacts remain rejected.
