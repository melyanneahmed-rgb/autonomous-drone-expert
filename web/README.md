# `web/` — canonical Web/PWA product package

This is the only approved root for the Web/PWA product package. No UI source, package
manifest or dependency lock is integrated in this policy-only change.

The next separately reviewed source-integration change may create this layout:

```text
web/
├── public/
├── src/
├── tests/
├── package.json
├── package-lock.json
├── tsconfig.json
└── vite.config.ts
```

`package.json` and `package-lock.json` must arrive together. Before either is accepted,
the exact lockfile SHA-256 must be recorded in `policy/web-dependencies.json` after a fresh
transitive, license, install-script, native-binary and vulnerability audit. The governing
package manager is `npm@11.13.0`; pnpm, Yarn, Bun, Deno, nested workspaces and packages
outside `web/` are rejected by `scripts/check_web_dependencies.py`.

## Product boundary

- TypeScript/React is the Web shell; Rust remains the deterministic product core.
- UI code consumes product contracts. It does not build protocol frames or own write
  authority.
- No IndexedDB, Web Serial, WASM binding, hardware or UI implementation is authorized by
  this gate.
- Android/APK source, wrappers and dependencies remain deferred and prohibited.

The approved Site v3 is a design/provenance reference, not a package template. Its hosting,
database, server-component and platform scaffolding must not be copied into this repository.
