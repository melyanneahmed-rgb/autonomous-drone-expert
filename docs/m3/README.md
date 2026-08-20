# M3 — Read-only capability-pack resolution

**Status:** slices 1–2 implemented for review

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

## Safety properties

The current M3 slices cannot represent or perform:

- MSP/CLI command ids or arbitrary payloads;
- a transport or device handle;
- a write approval;
- SET/SAVE/EEPROM/reboot/motor/arm/DFU/flashing authority;
- a wildcard target match;
- a signed/trusted distribution claim;
- `HARDWARE_SUPPORT_VALIDATED=YES`.

An invalid descriptor is a terminal typed error instead of being skipped. More than one matching
descriptor is `Ambiguous`; the resolver never chooses one implicitly. An unknown firmware family
is also terminal for capability selection.

## Distribution boundary

ADR-0007 requires distributed capability packs to be signed, checksummed, versioned and
revocable. M3 intentionally stops before that infrastructure. `ReviewOnlyEmbedded` means exactly
that: repository-reviewed descriptive data, unsuitable for external distribution and unable to
authorise a write. Signed-pack key management, revocation feeds and rollback pack selection
remain part of the later Knowledge Platform milestone.

## Next M3 work

1. Separate **readable identity profiles** from the still-exact M1 write scope so adding a known
   read-only firmware/API profile can never silently broaden write eligibility.
2. Review pinned official provenance for the next read-only identity profile before changing any
   decoder or command sequence.
3. Add capability-pack selection evidence to the Web diagnostic/result model without exposing
   firmware-engine details in the ordinary product UI.
4. Keep all real writes blocked until the later write milestone and a separate owner approval.

No physical operation is required for these slices.
