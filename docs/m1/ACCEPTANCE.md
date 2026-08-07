# M1 acceptance contract

M1 is accepted for owner testing only when one immutable Draft-PR head satisfies every
gate below. A green result on an earlier SHA is not evidence for a later SHA.

## Required CI jobs

- `policy-gates`
- `rust-linux`
- `rust-windows`
- `rust-msrv`
- `cargo-deny`
- `coverage`

Every job has a 60-minute timeout. Duplicate push/PR delivery for one SHA shares a
concurrency key and stale work is cancelled.

## Coverage gate

The `coverage` job pins `cargo-llvm-cov` and collects real branch counters. The policy
script requires all five files:

- `crates/casebook/src/lib.rs`
- `crates/core-api/src/lib.rs`
- `crates/execution/src/lib.rs`
- `crates/recovery/src/lib.rs`
- `crates/transport/src/lib.rs`

Aggregated thresholds are at least 90% lines and 70% branches. Missing files, malformed
counters and a zero branch total fail closed.

## Review checklist

- Existing typed M1 API and simulation semantics remain available.
- `unsafe` is still forbidden and external production dependencies remain prohibited.
- The first possible write has a proven backup and synced write-ahead event.
- Resume never converts missing identity evidence into a fresh-run abort.
- Complete journal corruption is rejected; only a torn final append is recoverable.
- An interrupted recovery never resumes as the normal apply path.
- Terminal rebuild validates exact target, bit mask and write-ahead recovery classes.
- Linux and Windows run the non-hardware example and print a successful `M1RunReport`.
- No hardware, release, support or readiness claim is introduced.
- The Draft PR remains unmerged until the owner runs the agreed user tests and decides.
