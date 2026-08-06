# ADR-0009 — First-Party Workspace Path Dependencies

- **Status:** Accepted — **for M1 integration only**. This ADR changes the dependency
  gate; it does not implement any lifecycle, transport, mock, replay or beeper logic, and
  it grants no hardware-write authority.
- **Date:** 2026-08-06

> **ملخص:** تنتهي سياسة «صفر اعتمادات مطلقة» في الأساس، وتُستبدل بقاعدة دقيقة: يُسمح فقط
> باعتمادات مسار (path) داخلية إلى crates أعضاء في نفس الـworkspace. تبقى كل الاعتمادات
> الخارجية (registry/git/URL) ممنوعة، ويُفرض ذلك محليًا ببوابة العزل وخارجيًا بـ`cargo-deny`
> كفحص مطلوب. القرار لا يغيّر الترخيص النهائي ولا قرار الـtransport ولا يمنح Hardware Write.

## Context

The foundation batch (M0) was approved with **zero dependencies**, enforced by
`scripts/check_isolation.py` under a `FOUNDATION_NO_DEPENDENCIES` flag that failed on any
non-empty dependency table. That flag was correct for a structural workspace with no code.

M1 is the first batch that writes real code across the established crate boundaries
(`ade-protocol-msp`, and the transport/session/safety/planning/execution/recovery/casebook/
mock-fc/core-api layers). Those layers must *use* one another — the orchestrator uses the
codec, the executor uses the transport contract, and so on. In Cargo that requires a path
dependency onto a sibling workspace crate. The absolute zero-dependency flag blocks exactly
this, and the only alternatives are worse: collapsing every layer into one crate (destroying
the architecture the design mandates) or duplicating code across crates.

The gate's own comment anticipated this: "Introducing the first dependency is its own
reviewed pull request that must also enable cargo-deny as a required check." This ADR and
its pull request are that step.

## Decision

Replace the absolute rule with a precise one. The dependency gate now permits **exactly one
shape** of dependency and rejects everything else.

### Permitted

1. First-party **path** dependencies only: `dep = { path = "../<workspace-crate>" }`.
2. The resolved path must stay **inside the repository root** (checked structurally with
   `Path.resolve()` + `is_relative_to`, never by string prefix).
3. The resolved path must be a **real member** of the current workspace (it matches a
   `workspace.members` glob and holds a `Cargo.toml` with a `[package].name`).
4. The **actual package name** at that path must match the dependency declaration — either
   the dependency key, or an explicit `package = "…"` rename. An alias that resolves to a
   different package is rejected.
5. Path dependencies are permitted in `dependencies`, `dev-dependencies`,
   `build-dependencies` and target-specific tables, subject to all of the above.

### Prohibited (unchanged posture — EXTERNAL PRODUCTION DEPENDENCIES REMAIN PROHIBITED)

- Any registry/version dependency (including a bare version string).
- Any `git` or URL dependency.
- Any wildcard (`*`) version or path.
- Any path escaping the repository, or a path to a directory that is not a workspace member.
- Any hybrid that carries a `path` **and** a `version`/`git`/`registry` key.
- Any `workspace = true` inherited dependency that cannot be resolved to a pinned local
  member.
- Any vendored source, submodule, or additional git remote.

### First-party coupling is not a third-party dependency

Depending on a crate **we wrote**, that lives **in this repository**, introduces no external
code and no supply-chain, license or vulnerability surface. It is architectural coupling,
not a supply-chain dependency. That distinction is the whole basis of this ADR.

## cargo-deny becomes a required check

`cargo-deny` is enabled as a **real, required** CI job (never advisory-only, never
`continue-on-error`), pinned to `cargo-deny 0.20.2` — the version already reviewed for the
Windows spike — installed with `cargo install --locked`. It runs `advisories`, `bans`,
`licenses` and `sources`.

`deny.toml` gains `[licenses] private = { ignore = true }` so our own unpublished
(`publish = false`) crates, which intentionally carry no license while the licensing
decision is deferred (ADR-0004), are skipped. This is **not** a third-party license
allowance and does not expand the allowlist. No license was added; no important warning was
downgraded to `allow`.

## Consequences and boundaries

- The first **external** dependency ever proposed still requires its own **Dependency
  Audit** and a separate reviewed pull request. This ADR does not pre-authorise any.
- This ADR does **not** change the final license decision (ADR-0004).
- This ADR does **not** choose or endorse any serial/transport backend, and does not move
  any spike dependency into production.
- This ADR grants **no** hardware-write authority; the M1 slice remains Mock/Replay only.
- The board and target `SPEEDYBEEF405V4` remain `PROPOSED — NOT HARDWARE VALIDATED`.
