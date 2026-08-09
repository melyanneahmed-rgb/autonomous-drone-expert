# Approved Smart Configurator Site v3 port map

## Authority

- Approved source commit: `4d6dbc801f67c79bfe172ded9a819e42d084fdc7`.
- Approved source file count: 35.
- Recovered ZIP SHA-256:
  `07637fb1ec448fc2c9184f2acc30af8ad708d0d09e5e834b888c4cd8772b48b1`.
- This map covers only the selectively ported UI/PWA source. No other repository or design
  source was used.

## Mapping

| Approved Site v3 source | Source SHA-256 | Repository destination | Status | Necessary adaptation | Visual effect |
| --- | --- | --- | --- | --- | --- |
| `app/page.tsx` | `0aa43ac4e7a2d0f626fae791aee10c490ad8037985e95c9ef07c3e4fe077aab1` | `web/src/App.tsx` | PORTED | Removed Next client directive; split type import; renamed component; replaced Web Serial chooser with truthful deferred state | NONE |
| `app/globals.css` | `baac7161c4ea69b58a3ddb5350a79fe8ad889bfa25b1eb3f6460a22f0a164cbb` | `web/src/styles.css` | PORTED | Removed the platform-only Tailwind import; all authored design CSS retained | NONE |
| `app/layout.tsx` | `0ea79c0b703aae32065d32fd0ae002a42b2ea9dcd21f9226168650684bc0403e` | `web/index.html` | PORTED | Next metadata/layout expressed as static HTML; removed development-only metadata | NONE |
| `app/pwa-register.tsx` | `da09b146ab8be00efdbc4d34769e7ca0a59d12865a7f21064da0a1b7d7a7257b` | `web/src/pwa-register.ts` | PORTED | React effect wrapper replaced by one browser registration function | NONE |
| `public/manifest.webmanifest` | `39113329fa9f63c43f78bcb19ace91a5c71e685bc13eb6c8af7a42082cec2558` | `web/public/manifest.webmanifest` | PORTED | None | NONE |
| `public/sw.js` | `e802c3d7878164711d125c0cb512082362722ad351ba42dd6e385eefdd383889` | `web/public/sw.js` | PORTED | None; same-origin GET-only cache boundary retained | NONE |
| `public/favicon.svg` | `e6d2e59b7b5bbb0342e0fb496dfc262decbfe4426bbb7b047aec8d467d1dc6f7` | `web/public/favicon.svg` | PORTED | None | NONE |
| React/Vite entrypoint | n/a | `web/src/main.tsx` | ADAPTER | Minimal `createRoot` bootstrap and PWA registration | NONE |
| Static build configuration | n/a | `web/tsconfig.json`, `web/vite.config.ts` | ADAPTER | Minimal pinned Option B compiler/bundler configuration | NONE |

## Retained visible authority

- Product identity, Arabic RTL direction and all initial visible copy are retained.
- Hero, typography, spacing, borders, cards, color system, responsive breakpoints, icons,
  status/privacy language and step order are retained.
- Authored CSS is retained without redesign or design-system extraction; the unresolved
  platform Tailwind import is the only removed line.
- Visible design deviations: 0.

## Capability boundary adaptation

`FUNCTIONAL CAPABILITY DEFERRED / VISUAL DESIGN PRESERVED`

The approved source's Web Serial chooser was not authorized for this gate. Both USB buttons
retain their visual design and initial copy, but activating them performs no browser device
request and reports that the capability is deferred. It never claims a successful device
selection. Local file selection and Web Crypto hashing remain local-only: no bytes, file
name, digest or metadata are uploaded, persisted or logged.

## Explicitly not ported

- `.openai/hosting.json`, Sites build plugin and Worker.
- Next/Vinext/React Server Components.
- Cloudflare/Wrangler and deployment configuration.
- Drizzle, D1/R2 and examples.
- Tailwind/PostCSS configuration.
- Site shell scripts, auth helpers and unrelated starter SVGs.
- IndexedDB, Web Serial, USB/HID/Bluetooth, WASM bindings, hardware or APK capability.
