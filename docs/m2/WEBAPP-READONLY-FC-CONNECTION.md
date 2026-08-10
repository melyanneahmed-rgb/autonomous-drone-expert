# M2 production WebApp read-only FC connection

Status: owner-review gate; software and real-Chrome exercised; physical FC not tested.

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
  -> exact Rust-owned four-read identity sequence
  -> privacy-bounded typed result
  -> Rust-directed cleanup and port close
```

`web/src/connection/readonly-fc-connection.mjs` is the complete public connection facade. It
prepares the audited Rust runtime before either product connection button is enabled. React
cannot choose a command, payload, baud rate, frame, response decoder, host implementation, or
WASM trust type. The accepted host at `web/src/transport/webserial-readonly-host.mjs` remains
byte-for-byte unchanged.

The UI keeps only an ephemeral typed result containing `apiVersion`, `fcVariant`, `fcVersion`,
`targetName`, `scopeMismatchField`, or a stable failure class. It does not request, display,
log, or persist a serial number, COM path, VID/PID, `getInfo()` result, UID, raw frame, permission
record, or unique board signature. A four-read scope mismatch is displayed as a mismatch.

## Generated product asset boundary

The production Vite copy boundary contains exactly:

- `web/public/wasm/ade_web_readonly_serial_wasm_bridge.js`
- `web/public/wasm/ade_web_readonly_serial_wasm_bridge_bg.wasm`

Their source, isolated generator, toolchain, sizes, and SHA-256 values are locked in
`policy/webserial-wasm-assets.json`. `scripts/verify_webserial_product_assets.py` validates the
committed copies. Trusted Linux CI rebuilds the bridge with Rust 1.85.0, runs the existing
isolated `wasm-bindgen-cli-support = 0.2.127` tool with Rust 1.97.1 into a temporary directory,
and compares both outputs byte-for-byte. The generated files are derived build output and add
no authority. Vite copies the same verified bytes to `dist/wasm/`, and the same-origin service
worker precaches only those exact two bridge assets.

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
if ($js -ne "68E2EE99C2C1A77E2D35B495672BEF6F8896A384A9C4F79E26599D14EC9FBE6C") { throw "JS glue hash mismatch" }
if ($wasm -ne "C497CDAB06207E6D034F0383F0F4E704C18BFBDE0A20F5DAF6EB8CD09615EAF7") { throw "WASM hash mismatch" }

Set-Location web
npx --yes npm@11.13.0 ci --ignore-scripts --no-fund
npx --yes npm@11.13.0 run build
py -3.13 -m http.server 8765 --bind 127.0.0.1 --directory dist
```

Open exactly: `http://127.0.0.1:8765/`

Before any later owner-controlled physical click: remove all propellers, disconnect LiPo,
connect the FC by USB only, do not press BOOT, do not enter DFU, do not flash firmware, close
Betaflight Configurator and SpeedyBee software, and do not attach USB-UART/FTDI or battery
power. The board label `SPEEDYBEEF405V4` is not trusted; the Rust result decides the typed
identity and any scope mismatch.

Evidence remains: `PHYSICAL_FC_NOT_TESTED`; `HARDWARE_OBSERVED=NO`.
