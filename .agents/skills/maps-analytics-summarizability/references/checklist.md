# Maps Analytics Summarizability Checklist

Use this checklist as a review aid.

- [ ] The task scope is explicit and narrow.
- [ ] Relevant Andromeda invariants are named.
- [ ] The proposed change is typed, bounded, versioned, observable, and recoverable where applicable.
- [ ] Security and audit impact are stated.
- [ ] Compatibility impact is classified as additive, behavior-impacting, security-impacting, or breaking.
- [ ] Failure modes are described.
- [ ] Tests or validation commands are named.
- [ ] Residual risks are not hidden.
- [ ] No SQL ad hoc application surface is introduced.
- [ ] No GPU/learned/predictive path is placed on commit, WAL, rollback, recovery, MVCC, or security-critical logic.
