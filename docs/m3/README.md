# M3 — Read-only capability-pack resolution

**Status:** slices 1–4 merged; strict API 1.47 read-only decoder in review

M3 begins the firmware capability-pack layer accepted by ADR-0007. This milestone does not add a
hardware write, a driver, a transport, a command table or a signed-pack distribution system.
Its current contracts are deliberately narrower: descriptive firmware knowledge can be
represented, validated and matched against authoritative decoded identity facts while remaining
unable to grant write authority.

## Starting point

M2 is merged to `main`. The production Web/PWA path can perform the Rust-owned read-only identity
sequence for the currently accepted API profile, and the integrated tree passed canonical CI and
Android development-validation packaging. Physical evidence remains bounded:

- `PHYSICAL_FC_TEST_ATTEMPTED=YES`
- `PHYSICAL_USB_SELECTION_OBSERVED=YES`
- `PHYSICAL_API_SCOPE_GATE_OBSERVED=YES`
- `UNSUPPORTED_API_OUTCOME_OBSERVED=YES`
- `READONLY_IDENTITY_COMPLETION=NO`
- `HARDWARE_SUPPORT_VALIDATED=NO`

M3 does not reinterpret or inflate those facts.

## Slice 1 — descriptive schema and fail-closed resolver

`ade-capability` models:

- internal firmware family;
- MSP protocol/API range;
- firmware-version range;
- exact target selector only — no wildcard target;
- schema version, pack revision and revocation identifier;
- an explicit `ReviewOnlyEmbedded` trust state;
- an explicit `WritesBlocked` policy with no write-enabled variant;
- a minimal privacy-bounded observed identity view;
- a resolver that validates every descriptor before matching and refuses both malformed and
  ambiguous knowledge.

The first embedded review descriptor describes only the legacy Betaflight 4.5.5 / MSP protocol 0
/ API 1.46 / `SPEEDYBEEF405V4` tuple. It is descriptive upstream knowledge, not a claim that the
owner's physical hardware is that target and not a hardware-support claim.

## Slice 2 — authoritative facts adapter

`ade-capability-resolution` is the narrow integration seam between `ade-facts::DeviceIdentity`
and the descriptive pack model. It depends only on the first-party facts and capability crates;
protocol types are used only by test fixtures.

The adapter:

- receives an already-decoded `DeviceIdentity`; it never receives raw MSP bytes;
- maps only the already-reviewed `BTFL` variant to `FirmwareFamily::Betaflight`;
- refuses every other variant as `UnrecognizedFirmwareFamily` rather than guessing;
- passes only firmware family, MSP protocol/API, firmware version and target name into capability
  resolution;
- deliberately drops board identifier, hardware revision, board name, manufacturer id and other
  fields not required to select firmware knowledge;
- returns a stable review-only status that preserves both `ReviewOnlyEmbedded` trust and
  `WritesBlocked` policy on every match;
- does not call or replace the separate M1 write-scope check.

This creates no path from a capability match to `WriteApproval`, transport exchange or a hardware
command.

## Slice 3 — pinned research for the next read-only profile

The clean-room provenance set for Betaflight `2025.12.1` at MSP protocol `0` / API `1.47` is now
recorded against official upstream commit `85d201376a1fc33b223c27448808c2cc7b8f2743` before any
decoder or identification-sequence change.

The repository records:

- `MSP_API_VERSION`: three bytes and the pinned protocol/API tuple;
- `MSP_FC_VARIANT`: fixed four-byte `BTFL` identifier;
- `MSP_FC_VERSION`: the 2025 calendar-version triplet followed by a one-byte length and the
  version-string bytes;
- `MSP_BOARD_INFO`: the complete documented variable-length field sequence through the API 1.47
  tail, including bounded pstrings and the 32-byte signature that must remain excluded from stable
  identity/persistence.

These are `PINNED_SOURCE_RECORDED / NOT_REPRODUCED` research facts only. They do not mean the
product identifies the owner's hardware as API 1.47 or Betaflight 2025.12.1, and they do not
broaden the M1 write scope.

## Slice 4 — readable profile versus write eligibility

`ade-readonly-profile` creates an explicit type boundary before API 1.47 production identification
is enabled.

It describes two exact read-layout candidates:

- protocol 0 / API 1.46: legacy three-byte `FC_VERSION`;
- protocol 0 / API 1.47: calendar triplet plus one-byte-length version string.

Both require the exact four-byte `BTFL` variant before profile acceptance. Unknown protocol/API
or variant combinations fail closed. Every candidate permanently carries
`NeverAuthorizesWrites`; there is no write-enabled variant.

A cross-crate regression test intentionally proves the separation: API 1.47 can be known to the
read-profile registry while `ade-facts::check_m1_api_scope` still rejects API 1.47 for the M1 write
scope. Future read support must not alter that fact accidentally.

## Slice 5 — strict profile-gated `FC_VERSION` decoder

The read-profile crate now depends only on the existing first-party MSP codec in production and
adds a decoder that cannot run a profile-specific layout before the exact firmware-variant gate
passes.

For API 1.46 it delegates to the existing strict three-byte `FcVersion` decoder unchanged.
For API 1.47 it accepts only:

1. the three published calendar-version bytes;
2. one `u8` version-string length;
3. exactly that many strict UTF-8 bytes;
4. no undocumented trailing payload.

The API 1.47 result keeps the raw calendar triplet and the strict version string separately; it
does not reinterpret the triplet into a semantic version or infer the owner's physical firmware.
Malformed prefixes, declared-length overruns, trailing bytes, invalid UTF-8, wrong command and
wrong direction all fail closed with bounded structural errors. An adversarial test also proves
that a non-`BTFL` variant is rejected before payload parsing starts.

This slice still does not change the Rust-owned production identification state machine, so it does
not yet turn the owner's previous `api-unsupported` observation into a completed identity.

## Safety properties

The current M3 slices cannot represent or perform:

- arbitrary MSP/CLI commands or arbitrary payload actions;
- a transport or device handle;
- a write approval;
- SET/SAVE/EEPROM/reboot/motor/arm/DFU/flashing authority;
- a wildcard target match;
- a signed/trusted distribution claim;
- `HARDWARE_SUPPORT_VALIDATED=YES`.

An invalid descriptor is a terminal typed error instead of being skipped. More than one matching
descriptor is `Ambiguous`; the resolver never chooses one implicitly. An unknown firmware family
or unreadable profile is also terminal for the respective read-only selection layer.

## Distribution boundary

ADR-0007 requires distributed capability packs to be signed, checksummed, versioned and
revocable. M3 intentionally stops before that infrastructure. `ReviewOnlyEmbedded` means exactly
that: repository-reviewed descriptive data, unsuitable for external distribution and unable to
authorise a write. Signed-pack key management, revocation feeds and rollback pack selection
remain part of the later Knowledge Platform milestone.

## Next M3 work

1. Integrate the reviewed API 1.47 read profile into the Rust-owned identification state machine so
   `MSP_API_VERSION` selects a read candidate, `MSP_FC_VARIANT` gates it, and only then the
   profile-specific `MSP_FC_VERSION` decoder runs.
2. Preserve the exact M1 write-scope check independently: API 1.47 must remain write-ineligible even
   if read-only identity becomes complete.
3. Add capability/profile selection evidence to the bounded Web diagnostic/result model without
   exposing firmware-engine details in the ordinary product UI.
4. Keep all real writes blocked until the later write milestone and a separate owner approval.

No physical operation is required for these slices.
