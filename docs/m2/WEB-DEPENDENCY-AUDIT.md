# M2 Web dependency audit

- **Initial candidate audit:** 2026-08-08 (contract-only; Site v3 reference lock)
- **Integration lock audit:** 2026-08-09 (fresh host-generated minimal lock)
- **Decision:** Option B — minimal audited Web stack, locked
- **Accepted lock SHA-256:** `c3015e9454da094d307975921b8aa2c195a15b9dffe0498a9c758b57d922c05d`
- **Network result:** the Codex sandbox remained registry-blocked, so the owner generated the
  fresh lock with npm 11.13.0 on the proven-working Windows host. Codex then independently
  verified the files, complete closure, digest and saved npm audit JSON before import.

## Accepted direct candidates

These exact versions are represented by the reviewed fresh lock. Vite 8.2.1 supersedes the
earlier 8.0.13 candidate before any package installation or UI integration.

| Class | Package | Exact version | License | Audited transitive count | Install/native/network/footprint assessment | Why needed |
| --- | --- | ---: | --- | ---: | --- | --- |
| Runtime | `react` | 19.2.6 | MIT | 0 | No install script or native binary; browser runtime only; no package-origin network at runtime | Approved component/state model |
| Runtime | `react-dom` | 19.2.6 | MIT | 1 | Pulls `scheduler`; no install script/native binary; browser renderer footprint | Renders React to the DOM |
| Development | `@types/node` | 22.19.19 | MIT | 1 | Types only; pulls `undici-types`; no runtime footprint | Types for build configuration |
| Development | `@types/react` | 19.2.14 | MIT | 1 | Types only; pulls `csstype`; no runtime footprint | Strict TSX checking |
| Development | `@types/react-dom` | 19.2.3 | MIT | 0 | Types only; no runtime footprint | Strict DOM renderer checking |
| Development | `typescript` | 5.9.3 | Apache-2.0 | 0 | Build-time compiler; no install script/native binary/runtime network | Deterministic strict checking |
| Development | `vite` | 8.2.1 | MIT | 40 | Build/dev only; Rolldown and Lightning CSS use audited optional platform binaries; dev server is local | Minimal TSX dev server and static production bundler |

The accepted fresh closure is 48 unique packages: seven direct and 41 unique transitives.
Three entries are production-reachable and 45 are development entries; 26 are optional.
License counts are: MIT 32, Apache-2.0 2, ISC 1, BSD-3-Clause 1 and MPL-2.0 12.

### Install scripts and native/prebuilt packages

Only `fsevents@2.3.3` carries `hasInstallScript` in the observed minimal closure. It is an
optional, development-only macOS watcher and is allowed only with that exact identity and
both lock markers. Vite 8 uses Rolldown and Lightning CSS optional binaries for supported
build hosts. The closure therefore contains platform-specific prebuilt packages for
Darwin, Linux, Windows, FreeBSD, OpenHarmony, Android and a WASM fallback. They execute only
as build tooling and do not enter the browser product.

The Android-named `@rolldown/binding-android-arm64@1.2.3` and
`lightningcss-android-arm64@1.33.0` artifacts are not APK dependencies. The gate allows
only these two exact optional/dev-only records observed in the accepted fresh lock and
rejects every direct Android/mobile dependency and every other Android-named transitive.

### Network and runtime behavior

Package installation needs HTTPS access to `registry.npmjs.org`; all 48 accepted closure
entries resolve there and carry SHA-512 integrity. React/React DOM require no product
network endpoint. Vite's dev server is build tooling only and must not be exposed to an
untrusted network. Production output is static browser code; the package manager and Vite
are not production services.

## Security review

- The 2026-08-08 policy historically evaluated Vite 8.0.13 as a contract-only candidate.
  A fresh host pre-integration audit later reported a HIGH advisory for that candidate, so
  it was rejected before installation and is not the accepted product lock.
- A new exact minimal lock was resolved with Vite 8.2.1. Its saved npm audit JSON reports
  zero info, low, moderate, high and critical vulnerabilities; Codex independently parsed
  that evidence and verified the lock pins exactly 8.2.1.
- The reviewed React Server Components denial-of-service advisory is patched in 19.2.6.
  This policy does not admit any `react-server-dom-*` package or server-component stack.
- No React, React DOM or TypeScript install hooks were present in the observed closure.
- The accepted digest is pinned in machine policy. Any package, version, dependency-class,
  install-script exception or byte-level lock change fails closed and requires a new audit.

References: [Vite 8 announcement](https://vite.dev/blog/announcing-vite8),
[Vite GHSA-p9ff-h696-f583](https://github.com/vitejs/vite/security/advisories/GHSA-p9ff-h696-f583),
[React GHSA-rv78-f8rc-xrxh](https://github.com/react/react/security/advisories/GHSA-rv78-f8rc-xrxh),
[TypeScript 5.9.3 release](https://github.com/microsoft/TypeScript/releases/tag/v5.9.3).

## Rejected Site v3 packages

| Package/group | Site version | Decision and platform replacement |
| --- | ---: | --- |
| `next`, `vinext`, `react-server-dom-webpack`, `@vitejs/plugin-rsc`, `eslint-config-next` | 16.2.6 / 0.0.50 / 19.2.6 / 0.5.26 / 16.2.6 | Rejected: no SSR, RSC or App Router is required. Static React/Vite shell replaces them. |
| `@cloudflare/vite-plugin`, `wrangler`, Site build plugin/Worker | 1.37.1 / 4.92.0 | Rejected: hosting-specific coupling. Static deployment adapter remains a later hosting decision. |
| `drizzle-orm`, `drizzle-kit`, D1/R2 scaffolding | 0.45.2 / 0.31.10 | Rejected: unused in Site v3 and no database is authorized. Future browser storage must pass its own gate. |
| `tailwindcss`, `@tailwindcss/postcss` | 4.2.1 / 4.2.1 | Rejected: the approved design is custom CSS; native CSS is the smaller replacement. |
| `@vitejs/plugin-react` | 6.0.2 | Rejected for now: Fast Refresh is convenience, not required for TSX transform/build. Add only after a separate need and audit. |
| `eslint` | 9.39.4 | Rejected in the minimum gate. Lint/tooling selection belongs to the source-integration test plan. |
| Site shell scripts and `.npmrc` | n/a | Rejected: contain Bash, Wrangler, Sites and artifact assumptions not owned by this repository. |

No Next, Vinext, Cloudflare, Drizzle, Tailwind, Android, APK or hardware dependency is
approved by this decision.
