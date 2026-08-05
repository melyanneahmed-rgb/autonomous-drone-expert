# Contributing

This repository is governed by an approved foundational document. Read
`docs/foundational/v1.1.md` before changing anything. Where this file and the foundational
document disagree, the foundational document wins.

## 1. Independence rules (non-negotiable)

This project is **independent**. It is not a fork, continuation or extraction of any other
repository.

- No submodules, no subtrees, no vendored copies of another repository.
- No `git` dependencies and no `path` dependencies that escape this repository.
- No remote other than this repository's own origin.
- No file, test, fixture, snapshot or configuration copied from any other project of the
  author or from any third party.
- Betaflight, INAV and their configurators may be **read and studied**. Their code,
  comments, tests, fixtures, generated tables, error strings and internal structure must
  **never** be copied or mechanically translated into this repository.

These rules are enforced by `scripts/check_isolation.py` in CI. The enforcement targets
real coupling — imports, paths, submodules, remotes and copied files. Naming another
project in prose or documentation is allowed and expected.

## 2. Source provenance (protocol layer)

Every protocol fact — command identifier, payload layout, setting name, unit, behavioural
condition — requires a record under `provenance/records/` **before** it may appear in code.

- The source must be a **pinned tag or commit**. A moving branch such as `master`, `main`
  or `HEAD` is rejected by CI.
- A record starts at `UNVERIFIED`, becomes `MOCK_VERIFIED` when reproduced against the mock
  flight controller, and only becomes `HARDWARE_VERIFIED` after observation on real
  hardware.
- Never mark a payload layout verified because it looks right. Verified means observed.

Any pull request touching `crates/protocol-msp/` or `crates/protocol-cli/` is a
**protocol change**: it requires matching provenance records and must not contain quoted
upstream material.

## 3. Safety and write authority

- No code may issue a write, save, reboot, flash or motor command outside the execution
  engine, the safety validator and a declared recovery class.
- Learned, community and language-model knowledge can rank, warn and prefer between safe
  options. It can never override a safety invariant, a board capability, a firmware limit,
  an observed fact or an official rule. See the authority hierarchy in the foundational
  document, section 4.
- Hardware writes are a separately approved gate. Do not add one because the code is
  "ready".

## 4. Branching and pull requests

- `main` is protected. All changes arrive through pull requests.
- One historical exception exists and will not be repeated: the bootstrap commit that
  created `main` (`174c711`), which added only `README.md`, `NOTICE.md` and `.gitignore`
  so that a branch and a pull request could exist at all.
- Branch names: `feat/*`, `fix/*`, `chore/*`, `docs/*`, `spike/*`, `provenance/*`.
- `spike/*` branches are experiments and are never merged.
- Never use force-push, `reset --hard`, `rebase` or `commit --amend` on a pushed branch.
- Keep a pull request to one reviewable intent.

## 5. Dependencies

There are currently **zero** production dependencies, and CI enforces that for the
foundation batch. Introducing the first dependency is its own pull request and must
include: the candidate, the exact version, its license, its maintenance status, the
platforms it supports, the risk, the alternative considered, and whether legal review is
required. `deny.toml` must be enabled as a required check in the same pull request.

## 6. Toolchain

- The exact toolchain is pinned in `rust-toolchain.toml`.
- The documented MSRV is `rust-version` in `Cargo.toml`. These are two different things
  and must not be conflated.
- Nightly is never the project toolchain. A future fuzzing job pins its own nightly
  separately.

## 7. Unsafe Rust

Every crate declares `#![forbid(unsafe_code)]`, enforced by CI. No exception has been
proven necessary yet, including in `transport`. If one ever becomes necessary, it is a
dedicated pull request with written justification, a narrow scope and explicit owner
review — never a quiet relaxation.

## 8. Honesty in claims

- Do not describe a capability as supported until it has been verified by the means that
  the foundational document requires.
- Do not describe an audit, a matrix entry or a recovery path as complete or guaranteed
  when it has not been validated on hardware.
- If a step was skipped, say so in the pull request.
