# M2 physical FC_VERSION mismatch investigation

Status: clean-room software investigation. Identity did not complete, no hardware is available to
this work, and no firmware, target, board, USB identifier, or hardware support is claimed.

## Observed

The owner supplied exactly these four evidence markers from one separately authorized USB-only
attempt against the accepted `c2ad3c0aa95c379a2b5a17e423979530eed0d05e` deployment:

- `PHYSICAL_FC_TEST_ATTEMPTED=YES`
- `PHYSICAL_USB_SELECTION_OBSERVED=YES`
- `READONLY_IDENTITY_COMPLETION=NO`
- `HARDWARE_SUPPORT_VALIDATED=NO`

The visible terminal diagnostic was exactly:

- failure class: `ProtocolIdentityFailure`
- origin/phase: `IDENTITY_STAGE`
- stage: `FC_VERSION`
- reason: `WrongLength`

This proves only that a read-only attempt reached the Rust-owned `FC_VERSION` identity stage and
the strict typed decoder stopped fail-closed because the correlated command-3 reply payload was
not exactly the expected length. A separately visible value `31` is not interpreted: repository
code does not independently establish its meaning.

## Proven from pinned official sources

No upstream code, comment, fixture, table, or test was copied. The facts below are clean-room
restatements from immutable official Betaflight sources.

| Official source | Resolved commit | MSP protocol/API | Relevant reply facts |
| --- | --- | --- | --- |
| Betaflight `4.5.5` | `4adbd3ef7cb546947600e5f747bd5453c9573063` | protocol `0`, API `1.46` | `MSP_API_VERSION` is three bytes: protocol, API major, API minor. `MSP_FC_VARIANT` is four identifier bytes. `MSP_FC_VERSION` is exactly three bytes: major, minor, patch. `MSP_BOARD_INFO` contains the fixed identifier/hardware-revision/type fields, length-prefixed target and board names, manufacturer ID, signature, MCU type, configuration state, sample rate, and configuration-problems byte that the existing strict parser expects. |
| Betaflight `2025.12.1` (first identified final 2025.12 tag) | `85d201376a1fc33b223c27448808c2cc7b8f2743` | protocol `0`, API `1.47` | `MSP_FC_VERSION` begins with three calendar-version bytes and then adds a one-byte string length plus the version-string bytes. For the nine-byte final string `2025.12.1`, the whole payload is 13 bytes. The relevant `MSP_BOARD_INFO` tail remains compatible in structure, but this project does not reach it for an unsupported API. |
| Betaflight `2025.12.5` | `7348054f268f0058574719c134e9f149565bb8ea` | protocol `0`, API `1.47` | The extended `MSP_FC_VERSION` form remains; `2025.12.5` also produces a 13-byte payload. |
| Current upstream inspected during this investigation | moving state used only to bracket persistence, not provenance | API `1.48` at inspection time | The extended `MSP_FC_VERSION` behavior remains. Product policy does not rely on this moving-state observation. |

The exact official API bump is commit
[`dc40b8f65526a526383ebdb5aaba755712c3fcae`](https://github.com/betaflight/betaflight/commit/dc40b8f65526a526383ebdb5aaba755712c3fcae),
which changed the API minor constant from 46 to 47. The exact identified `MSP_FC_VERSION` format
change is commit
[`21eba179396e240ccc548fb63c18e23ddf628cf3`](https://github.com/betaflight/betaflight/commit/21eba179396e240ccc548fb63c18e23ddf628cf3),
which added the length-prefixed version string after the three numeric version bytes. The first
identified final official tag carrying that newer format is
[`2025.12.1`](https://github.com/betaflight/betaflight/releases/tag/2025.12.1).

Pinned source references:

- [Betaflight 4.5.5 release](https://github.com/betaflight/betaflight/releases/tag/4.5.5)
- [4.5.5 MSP declarations](https://raw.githubusercontent.com/betaflight/betaflight/4.5.5/src/main/msp/msp_protocol.h)
- [4.5.5 MSP reply implementation](https://raw.githubusercontent.com/betaflight/betaflight/4.5.5/src/main/msp/msp.c)
- [2025.12.1 MSP declarations](https://raw.githubusercontent.com/betaflight/betaflight/2025.12.1/src/main/msp/msp_protocol.h)
- [2025.12.1 MSP reply implementation](https://raw.githubusercontent.com/betaflight/betaflight/2025.12.1/src/main/msp/msp.c)
- [Betaflight 2025.12.5 release](https://github.com/betaflight/betaflight/releases/tag/2025.12.5)

## Proven from the accepted product code

At accepted head `c2ad3c0…`, `ReadonlyIdentification::accept_response` parsed a structurally valid
API reply and unconditionally advanced from `API_VERSION` to `FC_VARIANT`. The WASM bridge called
the complete-identity `check_scope` only after `BOARD_INFO`. Therefore an unsupported but valid
API tuple could cause all four read requests before scope rejection and could encounter the strict
three-byte `FC_VERSION` decoder first. JavaScript and React did not and do not own compatibility
policy.

The typed diagnostic has a narrower meaning than a raw serial capture. A wrong command,
request-direction frame, error reply, checksum error, truncated frame, or oversized frame is
rejected by correlation/framing before a successful stage decode. `FC_VERSION/WrongLength`
therefore means the correlated `FC_VERSION` reply reached that typed decoder and its payload length
was not three. It does not reveal the rejected bytes or the actual payload length.

## Inferred, ranked hypotheses

1. **H1 — newer/different MSP API format: high confidence, not a hardware fact.** The observation
   is consistent with the official API-1.47 extended `FC_VERSION` payload, and the accepted product
   previously deferred its API-scope check. It does not prove the device runs Betaflight 2025.12.x.
2. **H4 — another firmware or fork implements command 3 differently: moderate confidence.** The
   same typed result could arise from any correlated command-3 reply whose payload length is not
   three. Identity did not establish the firmware family.
3. **H2 — API 1.46 with a nonconforming reply: low confidence but possible.** Official 4.5.5/API
   1.46 emits exactly three bytes, but the physical API reply was not retained or proven.
4. **H3 — framing/chunk/correlation defect: low confidence.** The accumulator is tested across
   complete, fragmented, byte-at-a-time, wrong-command, bad-checksum, truncated, oversized, and
   coalesced inputs. The stage-specific decoder result requires an accepted correlated frame, but
   the bounded diagnostic alone cannot rule out every transport defect.
5. **H5 — identity-state/parser defect independent of firmware: low confidence.** Deterministic
   tests accept the official three-byte layout and reject both shorter and longer payloads. A
   previously unknown defect remains logically possible because no raw physical capture is part of
   the evidence boundary.

## Safety decision

The product scope remains exactly Betaflight 4.5.5, MSP protocol 0 / API 1.46. The child change
moves the existing exact API predicate into shared Rust facts and applies it immediately after the
structurally valid `MSP_API_VERSION` reply:

- protocol 0 / API 1.46 continues through the unchanged maximum four empty-payload reads;
- any other structurally valid tuple returns a typed `api-unsupported` scope outcome, fabricates no
  identity, emits no second MSP request, and closes through the existing Rust-directed path;
- malformed API replies remain `ProtocolIdentityFailure`;
- the strict three-byte API-1.46 `FC_VERSION` parser is unchanged;
- API 1.47, API 1.48, trailing bytes, overrides, arbitrary commands, and any write authority are
  not added.

## Not yet known

The board, target, firmware family/version, API tuple, actual rejected payload length/content, and
whether the child would change the result on that physical device are not known. Hardware support
is not validated. No physical retry is requested by this investigation; any reviewed integration,
deployment, and single USB-only retry remain separate owner decisions.
