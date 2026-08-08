# M2 Web dependency audit

- **Audit date:** 2026-08-08
- **Decision:** Option B — minimal audited Web stack, contract-only
- **Reference lock:** verified Site v3 `package-lock.json` (lockfile v3)
- **Network result:** a fresh npm lock generation was blocked by the local registry network
  policy. No package was installed and no unverified lock was accepted.

## Accepted direct candidates

These versions reproduce the minimal relevant portion of the approved Site v3 stack. They
are recorded in the policy, but cannot enter the repository until a fresh minimal lock
closure is reviewed and its SHA-256 replaces the current null digest.

| Class | Package | Exact version | License | Audited transitive count | Install/native/network/footprint assessment | Why needed |
| --- | --- | ---: | --- | ---: | --- | --- |
| Runtime | `react` | 19.2.6 | MIT | 0 | No install script or native binary; browser runtime only; no package-origin network at runtime | Approved component/state model |
| Runtime | `react-dom` | 19.2.6 | MIT | 1 | Pulls `scheduler`; no install script/native binary; browser renderer footprint | Renders React to the DOM |
| Development | `@types/node` | 22.19.19 | MIT | 1 | Types only; pulls `undici-types`; no runtime footprint | Types for build configuration |
| Development | `@types/react` | 19.2.14 | MIT | 1 | Types only; pulls `csstype`; no runtime footprint | Strict TSX checking |
| Development | `@types/react-dom` | 19.2.3 | MIT | 0 | Types only; no runtime footprint | Strict DOM renderer checking |
| Development | `typescript` | 5.9.3 | Apache-2.0 | 0 | Build-time compiler; no install script/native binary/runtime network | Deterministic strict checking |
| Development | `vite` | 8.0.13 | MIT | 46 | Build/dev only; Rolldown and Lightning CSS use audited optional platform binaries; dev server is local; about 15 MB larger than Vite 7 upstream | Minimal TSX dev server and static production bundler |

The combined closure observed in the verified Site lock is 56 unique packages including
the seven direct candidates (49 unique transitives). License counts are: MIT 39,
Apache-2.0 2, ISC 1, BSD-3-Clause 1, 0BSD 1 and MPL-2.0 12.

### Install scripts and native/prebuilt packages

Only `fsevents@2.3.3` carries `hasInstallScript` in the observed minimal closure. It is an
optional, development-only macOS watcher and is allowed only with that exact identity and
both lock markers. Vite 8 uses Rolldown and Lightning CSS optional binaries for supported
build hosts. The closure therefore contains platform-specific prebuilt packages for
Darwin, Linux, Windows, FreeBSD, OpenHarmony, Android and a WASM fallback. They execute only
as build tooling and do not enter the browser product.

The Android-named Rolldown and Lightning CSS artifacts are not APK dependencies. The gate
allows only the two exact optional/dev-only records observed in the reference lock and
rejects every direct Android/mobile dependency and every other Android-named transitive.

### Network and runtime behavior

Package installation needs HTTPS access to `registry.npmjs.org`; all 56 observed closure
entries resolve there and carry SHA-512 integrity. React/React DOM require no product
network endpoint. Vite's dev server is build tooling only and must not be exposed to an
untrusted network. Production output is static browser code; the package manager and Vite
are not production services.

## Security review

- GitHub's reviewed Vite arbitrary-file-read advisory affects Vite 8.0.0–8.0.4 and is
  patched in 8.0.5; the candidate 8.0.13 is beyond that fixed version.
- The reviewed React Server Components denial-of-service advisory is patched in 19.2.6.
  This policy does not admit any `react-server-dom-*` package or server-component stack.
- No React, React DOM or TypeScript install hooks were present in the observed closure.
- A live `npm audit` and newly resolved minimal closure could not be completed under the
  local registry network restriction. Therefore the lock digest remains null and source
  integration fails closed. A later integration must repeat advisory review against the
  exact new lock; this document does not claim the future ecosystem is vulnerability-free.

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
