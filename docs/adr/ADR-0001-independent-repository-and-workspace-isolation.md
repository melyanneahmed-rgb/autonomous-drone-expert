# ADR-0001 — Independent Repository and Workspace Isolation

- **Status:** Accepted
- **Date:** 2026-08-05
- **Governs:** every file in this repository

> **ملخص:** هذا المشروع مستقل بالكامل: مستودع وقاعدة كود وبنية وتطبيق ومنتج نهائي مستقلة.
> لا يُشتق من أي مشروع سابق ولا يستورد منه، والعزل مفروض بفحوص آلية لا بالنية.

## Context

The product is a new, independent engineering platform. Independence is a hard requirement
from the owner, not a preference: the repository, codebase, architecture, desktop
application and end product must stand alone. Two failure modes must be made structurally
impossible rather than merely discouraged:

1. Silent coupling to any pre-existing repository of the owner.
2. Derivation from Betaflight, INAV or their configurators, all of which are GPL-3.0.

## Decision

1. `melyanneahmed-rgb/autonomous-drone-expert`, private, default branch `main`, is the only
   repository for this project. No alternative repository is ever substituted, not even
   temporarily, and not even if repository creation fails.
2. Work happens exclusively in a workspace whose only source is this repository, never
   inside or beside another project's directory.
3. Prohibited by policy: submodules, subtrees, vendored copies, `git` dependencies,
   `path` dependencies escaping the repository, additional git remotes, and any file copied
   from another project. **CI enforces the structural half of that list** — dependencies,
   escaping paths, submodules, remotes and known vendored directory names.
4. Betaflight and INAV may be **read and studied**. Their code, comments, tests, fixtures,
   generated tables, error strings and internal structure are never copied or mechanically
   translated. Protocol facts are re-implemented independently and recorded under
   `provenance/` (ADR-0004).
5. Isolation checks target **structural coupling** — dependencies, escaping paths,
   submodules, remotes, vendored directory names. They do **not** detect copied or derived
   source: there is no similarity check and no hash comparison, and claiming otherwise
   would be a false assurance. Copying is controlled by the provenance policy (ADR-0004)
   and by human review. Naming another project in documentation is explicitly allowed. An
   earlier draft of this gate failed builds merely for mentioning a project name; that
   check was withdrawn as both useless and hostile to honest documentation.

## Alternatives rejected

- **Monorepo inside an existing project.** Rejected: violates the independence requirement
  and permanently entangles licensing and history.
- **Fork of an existing configurator.** Rejected: inherits GPL-3.0 obligations and an
  architecture built around a different product thesis (the user is the expert), which is
  precisely what this product rejects.
- **Trust-based isolation with no automated checks.** Rejected: coupling is introduced by
  accident far more often than by intent.

## Consequences

- Zero reuse in the short term; every layer is built from scratch.
- The project keeps full freedom in its final licensing decision (ADR-0004).
- CI must carry an isolation gate from the very first commit, before there is any code that
  could violate it.
- Because that gate cannot detect derivation, provenance discipline and human review are
  the real control against copying — not the pipeline.
