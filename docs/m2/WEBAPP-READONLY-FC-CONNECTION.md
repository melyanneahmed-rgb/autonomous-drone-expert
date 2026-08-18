# M2 production WebApp read-only FC connection

Status: typed-diagnostic owner-review gate; one physical USB-only attempt observed selection
and fail-closed identity rejection, but identity did not complete and hardware support is not
validated.

This milestone replaces the approved Smart Configurator UI's deferred USB action with the
accepted read-only Web Serial identity flow. It does not add configuration, backup, save,
reboot, motor, CLI, DFU, flashing, WebUSB, WebHID, or hardware-write authority.

## Product authority path

```text
React explicit click
  -> prepared ReadonlyFcConnection.selectPortFromUserGesture()
  -> accepted WebSerialReadonlyHost.selectPortFromUserGesture()
  -> ReadonlyFcConnection.discover()
  -> accepted WebSerialReadonlyHost.discover() with zero arguments
  -> Rust-owned prefix of the fixed four-read identity sequence
  -> privacy-bounded typed result
  -> Rust-directed cleanup and port close
```

`web/src/connection/readonly-fc-connection.mjs` is the complete public connection facade. It
prepares the audited Rust runtime before either product connection button is enabled. React
cannot choose a command, payload, baud rate, frame, response decoder, host implementation, or
WASM trust type. The accepted host at `web/src/transport/webserial-readonly-host.mjs` changed
only to forward the two Rust-owned, allowlisted diagnostic fields.

The UI keeps only an ephemeral typed result containing `apiVersion`, `fcVariant`, `fcVersion`,
`targetName`, `scopeMismatchField`, a stable failure class, or the bounded `failureStage` and
`failureReason` categories. It does not request, display, log, or persist a serial number, COM
path, VID/PID, `getInfo()` result, UID, raw frame, permission record, or unique board signature.
An unsupported but structurally valid API is rejected by Rust after the first read, before any
variant, FC version, or board request. A supported API can proceed through all four reads; a later
complete-identity scope mismatch is displayed as a mismatch.

## Generated product asset boundary

The production Vite copy boundary contains exactly:

- `web/public/wasm/ade_web_readonly_serial_wasm_bridge.js`
- `web/public/wasm/ade_web_readonly_serial_wasm_bridge_bg.wasm`

Their source, isolated generator, toolchain, sizes, and SHA-256 values are locked in
`policy/webserial-wasm-assets.json`. `scripts/verify_webserial_product_assets.py` validates the
committed copies. Trusted Linux CI rebuilds the bridge with Rust 1.85.0, runs the existing
isolated `wasm-bindgen-cli-support = 0.2.127` tool with Rust 1.97.1 into a temporary directory,
and compares both outputs byte-for-byte. The Rust build remaps the three embedded source paths
to stable `/source/` labels, and the generator removes only the non-executable host-specific
WASM name section. The generated files are derived build output and add no authority. Vite
copies the same verified bytes to `dist/wasm/`, and the same-origin service worker precaches
only those exact two bridge assets.

The pre-generation Rust WASM file is an unpublished intermediate whose SHA-256 is reported for
each trusted build, not claimed as a cross-host-stable artifact. Its host-specific metadata is
discarded by the pinned generator. Reproducibility is enforced on both committed product outputs,
along with exact source, manifest, toolchain, generator source and generator lock provenance.

No npm package, Rust runtime dependency, GitHub Action, CDN, or hosted runtime script is added.

## Windows owner launch procedure (after PR approval only)

Do not run this physical procedure until the owner has reviewed the final Draft PR. It does
not build or run the isolated wasm-bindgen generator, and it needs neither WSL nor Docker.

```powershell
Set-Location "$HOME\Documents\autonomous-drone-expert"
git fetch origin feat/m2-webapp-readonly-fc-connect
git switch feat/m2-webapp-readonly-fc-connect
git pull --ff-only origin feat/m2-webapp-readonly-fc-connect

$expectedHead = git rev-parse origin/feat/m2-webapp-readonly-fc-connect
$actualHead = git rev-parse HEAD
if ($actualHead -ne $expectedHead) { throw "HEAD does not match the reviewed remote branch" }
$expectedTree = git rev-parse "$expectedHead`^{tree}"
$actualTree = git rev-parse "HEAD`^{tree}"
if ($actualTree -ne $expectedTree) { throw "Tree does not match the reviewed remote branch" }
git status --short

$js = (Get-FileHash web/public/wasm/ade_web_readonly_serial_wasm_bridge.js -Algorithm SHA256).Hash
$wasm = (Get-FileHash web/public/wasm/ade_web_readonly_serial_wasm_bridge_bg.wasm -Algorithm SHA256).Hash
if ($js -ne "C383A32030CB6E361BED425BE8A7EE1B1872FC8D270B9FDB90B15AB1D6D59F75") { throw "JS glue hash mismatch" }
if ($wasm -ne "674E156F0F3AFD8D6FBAAE32A2D6D5D5BB1FF90387C9C553AAD50A6BDD61BDE2") { throw "WASM hash mismatch" }

Set-Location web
npx --yes npm@11.13.0 ci --ignore-scripts --no-fund
npx --yes npm@11.13.0 run build
py -3.13 -m http.server 8765 --bind 127.0.0.1 --directory dist
```

Open exactly: `http://127.0.0.1:8765/`

Before any later owner-controlled physical click: remove all propellers, disconnect LiPo,
connect the FC by USB only, do not press BOOT, do not enter DFU, do not flash firmware, close
Betaflight Configurator and SpeedyBee software, and do not attach USB-UART/FTDI or battery
power. A visible board label is not trusted; the Rust result decides the typed identity and any
scope mismatch.

## Owner observation — first USB-only attempt (2026-08-11)

The owner ran the actual production-built Smart Configurator once against an FC, with propellers
removed, LiPo disconnected, USB power only, no BOOT/DFU,
no flashing, no battery or motor test, no USB-UART/FTDI, and other configurators closed.
The WebApp loaded, initialized the audited WASM bridge, reached the ready state, obtained one
explicit browser selection, and then stopped fail-closed with `ProtocolIdentityFailure`.
There was no related JavaScript/WASM exception and no configuration change. The unrelated
deprecated Apple PWA meta-tag warning is not identity evidence.

This observation does **not** confirm target, firmware or API, establish scope, or validate
hardware support. Runtime
`hardwareObserved` remains `false`. The evidence mapping is:

- `PHYSICAL_FC_TEST_ATTEMPTED = YES`
- `PHYSICAL_USB_SELECTION_OBSERVED = YES`
- `READONLY_IDENTITY_COMPLETION = NO`
- `TERMINAL_RESULT = ProtocolIdentityFailure`
- `FAILURE_ORIGIN = IDENTITY_STAGE`
- `FAILURE_STAGE = FC_VERSION`
- `FAILURE_REASON = WrongLength`
- `HARDWARE_SUPPORT_VALIDATED = NO`

## Typed diagnostic follow-up

The Rust bridge now preserves only two bounded enum-like facts when a structural identity
failure occurs: the current stage (`API_VERSION`, `FC_VARIANT`, `FC_VERSION`, or
`BOARD_INFO`) and an allowlisted structural reason. It discards all numeric error details,
raw frames, payload content and invalid text bytes. The parser, response correlation, canonical
four-command maximum, strict UTF-8, length checks, trailing-payload refusal and cleanup behavior
are unchanged. React only displays the two Rust-produced labels for the current
session and never derives or persists them.

The investigation is recorded in `docs/m2/PHYSICAL-FC-VERSION-MISMATCH-INVESTIGATION.md`. The
minimal child gate distinguishes a malformed API reply from a structurally valid API outside
product scope. Rust accepts only protocol 0 / API 1.46 for continued identification. Any other
valid API tuple returns the bounded `api-unsupported` outcome, emits no second request, fabricates
no identity, keeps `hardwareObserved=false`, and closes through the existing cleanup path. The
strict three-byte API-1.46 `FC_VERSION` decoder is unchanged; no newer API is supported.

Do not repeat the physical test until the child Draft-PR head has passed all software,
reproducibility and CI gates, the owner has reviewed and separately integrated it, and a reviewed
deployment exists. The next test is
exactly one USB-only read attempt with propellers removed, LiPo and battery disconnected, FC
USB only, no BOOT, no DFU, no flashing, no motor action, no USB-UART/FTDI, and Betaflight and
SpeedyBee software closed. Record only the displayed failure class, stage and reason; do not
record USB identifiers or raw data.
