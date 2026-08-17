# GitHub-native delivery design

Status: infrastructure design for owner review. This does not change flight-controller authority.

## Scope and owner flow

The delivery path is GitHub-native:

1. a reviewed branch commit receives a successful canonical `CI` run;
2. the owner dispatches `Web Preview (GitHub Pages)` for an explicit ref;
3. the workflow resolves that ref to one immutable commit, proves successful CI for that exact commit, rebuilds and tests the production artifact, and deploys it;
4. the owner dispatches `Android APK Validation` for an explicit ref;
5. the workflow applies the same exact-commit CI prerequisite and publishes a hashed debug APK artifact.

Neither workflow pushes, merges, signs a release, obtains write authority over repository contents, or receives repository secrets. Both are manual-only. The expected Pages URL is:

`https://melyanneahmed-rgb.github.io/autonomous-drone-expert/`

## Coherent Web base-path model

`ADE_WEB_BASE_PATH` is the sole production base-path input. `web/vite.config.ts` normalizes it to an absolute path ending in `/` and supplies it as Vite's `base`. Vite therefore emits the correct HTML asset paths for `/` and `/autonomous-drone-expert/` from the same source.

All non-bundled runtime assets follow that same base:

- HTML uses Vite's `%BASE_URL%` for the entry module, manifest, and favicon.
- The manifest uses document-relative `./` values for start URL, scope, and icon.
- The read-only Web Serial host retains its existing virtual module boundary; the Vite resolver maps that module to the same-origin WASM bridge below the configured base.
- Both generated Rust bridge modules and binaries are staged under `web/public/wasm/` by `scripts/prepare_web_wasm.py` and are copied into the production artifact. Nothing is loaded from a CDN or third-party origin.
- The service worker derives its application root from its registered scope. Registration uses `import.meta.env.BASE_URL`; shell URLs, navigation fallback, and resource-cache messages remain inside that scope.
- Browser smoke servers accept an explicit base path so the existing IndexedDB, storage-WASM, and read-only Web Serial tests run at both supported scopes.

The production browser gate serves the real Vite artifact below the requested path and uses Chrome DevTools Protocol to prove that HTML, built JS/CSS, the manifest, both generated WASM bridge modules, both WASM binaries, application initialization, worker scope, and offline reload all work. It also records every HTTP request and fails if a repository-path build requests `/wasm/...`.

## Deterministic PWA updates

`ADE_BUILD_SHA` is embedded into each production build and appended to the service-worker URL. Its exact commit value names the cache `smart-configurator-shell-<commit>`.

The worker preserves offline-first behavior while preventing incompatible old/new runtime mixtures:

- navigation and mutable manifest/WASM resources are network-first with a commit-cache fallback;
- content-hashed Vite assets are cache-first;
- installation populates the commit-specific application shell;
- activation claims clients and deletes older application cache versions;
- registration bypasses the HTTP cache when checking the worker.

The production delivery test builds an old commit identity and a new commit identity, changes the server in place, and verifies that the new app becomes active and the old cache is removed. The same transition is exercised at `/` and `/autonomous-drone-expert/`.

## Exact CI provenance

`scripts/require_successful_ci.py` accepts only a 40-character lowercase commit SHA. With `actions: read` it queries `.github/workflows/ci.yml` for that exact `head_sha`, accepts only the canonical workflow name/path and safe event types, and requires the latest matching attempt to be completed successfully. It then obtains the Git tree SHA from GitHub. Missing, ambiguous, running, cancelled, or failed provenance stops the delivery workflow before any build or deployment.

The checkout has credential persistence disabled. The Web build job has `actions: read` and `contents: read`; only its dependent deployment job receives the officially required `pages: write` and `id-token: write`. The Android job has only `actions: read` and `contents: read`.

## Selected GitHub Actions policy

The repository permits only explicit selected actions. `policy/github-actions-allowlist.json` separates three scopes:

- `canonical_ci_temporary_exceptions`: the single temporary canonical-CI tag `actions/checkout@v7.0.1`;
- `delivery_actions`: the six immutable full-SHA references written directly in the Web Preview and Android workflow files;
- `delivery_transitive_actions`: immutable nested action references demonstrated from those pinned direct manifests.

Delivery contract tests require the direct Web Preview and Android `uses:` set to match `delivery_actions` exactly, require every direct and transitive delivery reference to be a 40-character SHA, reject wildcards, and fail if the canonical-CI exception spreads into either delivery scope. The canonical CI workflow remains constrained to its single declared exception.

### Demonstrated transitive selected-action enforcement

[Web Preview run #2](https://github.com/melyanneahmed-rgb/autonomous-drone-expert/actions/runs/32070083242) failed during GitHub's `Set up job` step, before checkout, because the selected-actions policy did not yet include `actions/upload-artifact@bbbca2ddaa5d8feaa63e36b76fdaad77386f024f`.

The pinned direct action `actions/upload-pages-artifact@fc324d3547104276b827a68afc52ff2a11cc49c9` is a composite action. Its exact `action.yml` (blob `82713b4cbb35ccf98dfa8f2e0deedc5e83b6d845`) contains at line 84:

`uses: actions/upload-artifact@bbbca2ddaa5d8feaa63e36b76fdaad77386f024f # v7.0.0`

GitHub selected-actions enforcement therefore applies to the nested immutable action as well as the direct workflow reference. The nested ref is recorded separately; it does not replace the direct Android artifact action `actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a`.

### Exact pinned-manifest audit

| Selected delivery action | Manifest kind | Nested `uses:` audit |
| --- | --- | --- |
| `actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1` | Node 24 (`action.yml` blob `5b0524f730db83f9513c18ab31a6c086c7239076`) | none |
| `actions/configure-pages@45bfe0192ca1faeb007ade9deae92b16b8254a0d` | Node 24 (`action.yml` blob `7ea02ee862bd1315b0be31625082750aa52ff896`) | none |
| `actions/deploy-pages@cd2ce8fcbc39b97be8ca5fce6e763baed58fa128` | Node 24 (`action.yml` blob `113b639c9d6a6a0ab2e5fa904ea6949968e78be2`) | none |
| `actions/setup-java@b6effb05e454b25005698d916606bdc6ffcbf961` | Node 24 (`action.yml` blob `396e53c708fa14eaf6f45eaa50e8ae499c1aa498`) | none |
| `actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a` | Node 24 (`action.yml` blob `7cb4d1e81db55320b41217e1a78a1a46e3d2baef`) | none |
| `actions/upload-pages-artifact@fc324d3547104276b827a68afc52ff2a11cc49c9` | composite (`action.yml` blob `82713b4cbb35ccf98dfa8f2e0deedc5e83b6d845`) | exactly `actions/upload-artifact@bbbca2ddaa5d8feaa63e36b76fdaad77386f024f` |
| `actions/upload-artifact@bbbca2ddaa5d8feaa63e36b76fdaad77386f024f` | transitive Node 24 (`action.yml` blob `7cb4d1e81db55320b41217e1a78a1a46e3d2baef`) | none |

No other nested `uses:` dependency appears in the exact pinned manifests, so no other action is added. The repository Actions setting remains owner-controlled and must be updated separately with this one exact transitive ref; enabling broad GitHub-created, verified-creator, Marketplace, or wildcard access is neither required nor intended.

## Pages availability and confidentiality boundary

GitHub's current documentation makes plan and repository visibility relevant to Pages availability, and a Pages site may be publicly reachable even when its source repository is private. The GitHub integration used during implementation could read the private repository but received `403 Resource not accessible by integration` from the Pages API, so repository-specific Pages enablement cannot be truthfully pre-certified here. The first official deployment is the fail-closed availability check.

If it fails only because the publishing source is not configured, the sole owner action is:

`Repository → Settings → Pages → Build and deployment → Source → GitHub Actions`

No local command is required. The workflow does not weaken repository visibility or security settings. References: [GitHub Pages availability](https://docs.github.com/en/pages/getting-started-with-github-pages/about-github-pages), [custom GitHub Actions publishing](https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site), and [official custom workflow guidance](https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages).

Because deployment is a public-artifact boundary, only `web/dist` is uploaded. The build and tests fail closed on same-origin policy. `scripts/check_public_web_artifact.py` permits only the expected deployable types, requires both WASM bridges, rejects source maps, validates WASM headers, and scans text assets for credential, device-identifier, and captured-trace patterns. No repository metadata, source checkout, logs, tokens, credentials, serial numbers, USB/device identifiers, captured traffic, or raw MSP bytes are copied into it.

## Android packaging architecture

The Android project is an isolated thin launcher built with Android Browser Helper 2.7.2. It opens the same hosted Smart Configurator PWA through the official Trusted Web Activity launcher and retains the library's Custom Tab fallback. This keeps one product UI and does not import Web build tooling into `web/package.json` or `web/package-lock.json`.

Chrome's Trusted Web Activity model requires Digital Asset Links at the PWA origin and a stable application signing certificate. A repository Pages project cannot supply an origin-root `/.well-known/assetlinks.json` from its repository subpath, and this milestone deliberately has no stable production signing key. The APK therefore makes no verified-fullscreen TWA claim; the Custom Tab fallback is the honest validation behavior. It also makes no claim that Web Serial or USB flight-controller access works on Android. References: [Chrome TWA integration guide](https://developer.chrome.com/docs/android/trusted-web-activity/integration-guide), [Digital Asset Links verification](https://developer.chrome.com/docs/android/trusted-web-activity/whats-new/#digital-asset-links), and [Android Browser Helper](https://github.com/GoogleChrome/android-browser-helper).

The manifest requests only `android.permission.INTERNET`. It contains no native code, WebView, USB library, MSP implementation, generic transport, write authority, signing material, or production key. The artifact uses the Android debug signing mechanism solely to create an installable development build and is always reported as `DEVELOPMENT / VALIDATION — NOT PRODUCTION SIGNED`.

## Android dependency, license, and reproducibility review

The dependency policy is machine-readable in `android/dependency-policy.json`:

- Eclipse Temurin JDK `17.0.20+8`, GPL-2.0 with Classpath Exception;
- checksum-verified Gradle `9.4.1`, Apache-2.0;
- Android Gradle Plugin `9.2.1`, Apache-2.0;
- Android Browser Helper `2.7.2`, Apache-2.0.

The runtime closure contains 53 exact Apache-2.0 modules. The full Android configuration lock contains 240 exact component versions, and Gradle dependency verification records SHA-256 evidence for 377 resolved build/runtime component versions in `android/gradle/verification-metadata.xml`. The build-only closure's reviewed license families are Apache-2.0, BSD-2-Clause, BSD-3-Clause, CDDL-1.1, EPL-1.0, GPL-2.0 with Classpath Exception, LGPL-2.1-or-later, and MIT; reciprocal-license build tools are not linked into the APK. Repository resolution is limited to Google's Maven repository, Maven Central, and the Gradle Plugin Portal, project repositories are rejected, dynamic versions are prohibited, and no opaque Gradle wrapper JAR is committed. The workflow downloads the official Gradle ZIP over HTTPS, verifies its pinned SHA-256, and then builds with dependency locking and strict verification.

`scripts/check_android_validation.py` fails CI on version, repository, lock, checksum, runtime-closure, permission, launcher URL, authority, signing-material, or wrapper-binary drift.

## Non-authority statement

This infrastructure leaves all Rust and physical flight-controller authority unchanged. It introduces no `SET`, `SAVE`, EEPROM write, reboot, motor, arm, CLI, DFU, flashing, restore, arbitrary MSP, generic payload API, generic transport effect, write approval, WebUSB, WebHID, or native Android USB authority. The Diagnostic Trace Panel is not part of this work.
