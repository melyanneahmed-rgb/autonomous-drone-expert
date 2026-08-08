# ADR-0011 — Audited Web dependency and repository policy

- **Status:** Accepted
- **Date:** 2026-08-08
- **Supersedes:** ADR-0002 for the JavaScript package manager and `ui/` package path.
- **Complements:** ADR-0009; it does not relax the Rust dependency policy.

## Context

ADR-0009 deliberately keeps production Rust external-dependency-free: only first-party
path dependencies between real members of this Cargo workspace are permitted. ADR-0010
then made Web/PWA the primary product shell. Treating Web build dependencies as if they
were Rust production dependencies would either block the approved shell or silently weaken
the safety-core policy. Both outcomes are wrong.

The recovered Smart Configurator Site v3 proves the approved interaction design, but its
Next/Vinext/Cloudflare/Drizzle/Tailwind scaffold is broader than this repository needs. It
also has its own hosting and server conventions. Copying that scaffold would import
platform coupling rather than preserve the design.

## Decision: Option B, minimal audited Web stack

The repository has four independent dependency classes:

1. **Rust production core:** unchanged ADR-0009 rules. First-party workspace paths only;
   registry, git, URL and external paths remain prohibited.
2. **Web production runtime:** exact audited React and React DOM versions only.
3. **Web development/build:** exact audited TypeScript, Vite and type-package versions
   only. These do not enter the browser runtime bundle as package-manager dependencies.
4. **Android/APK:** deferred. Direct Android, React Native, Expo, Capacitor, Cordova,
   Ionic and NativeScript dependencies are prohibited.

`npm@11.13.0` with package-lock format 3 is the sole manager. `web/` is the sole package
root. Direct versions are exact (no ranges or tags), and the complete reviewed lockfile is
governed by a SHA-256 recorded in `policy/web-dependencies.json`.

This change is deliberately **contract-only**: it adds neither `web/package.json` nor a
lockfile. The approved lock digest is null, so the gate refuses any Web source integration
until a future review adds both files and pins the newly audited lock digest. Listing a
candidate direct dependency is therefore not permission to install an unreviewed closure.

## Machine enforcement

`scripts/check_web_dependencies.py` is a standard-library-only, fail-closed gate. It:

- refuses packages outside `web/` and every other package manager;
- requires manifest and lockfile together;
- requires the exact manager, direct package set, dependency class and version;
- rejects ranges, tags, aliases, git/URL/tarball/local/workspace sources and links;
- requires an exact approved lock digest, registry HTTPS sources and SHA-512 integrity;
- enforces the reviewed license set;
- refuses root lifecycle hooks, arbitrary scripts and unreviewed transitive install hooks;
- refuses Android/mobile dependencies. The only Android-named exceptions are exact,
  optional, development-only cross-platform build binaries already observed in Vite's
  lock closure; they grant no Android product surface.

The gate runs as its own required step in the existing `policy-gates` CI job. It does not
run an npm install or Web build while the package is absent, and it does not weaken any
Rust, cargo-deny, MSRV, WASM or coverage job.

## Rejected alternatives

- **Option A — no Web dependencies:** incompatible with the approved React shell.
- **Copy Site v3's full stack:** rejected because Next/Vinext, Cloudflare, Drizzle,
  Tailwind, server components and hosting scripts are not required for a static local-first
  product shell.
- **pnpm workspace:** superseded. One canonical npm package avoids two manager/lock
  authorities before the Web source exists.
- **Loose semver ranges or unpinned lock:** rejected because review would not identify the
  code CI and developers actually install.
- **Unrestricted lifecycle scripts:** rejected because install-time execution is a separate
  code-execution capability, not ordinary dependency resolution.

## Consequences

- Web dependencies do not become Rust dependencies and cannot weaken ADR-0009.
- A dependency upgrade, class move, manager change, install-script exception or lock digest
  change requires an explicit policy edit and review.
- Site v3 design source may be ported selectively in a later UI gate, but its ZIP,
  `.openai/` hosting metadata and unused platform scaffolding are not repository source.
- This decision starts no UI, IndexedDB, Web Serial, WASM binding, APK or hardware work.
