# M3 — Read-only capability-pack resolution

**Status:** first slice in progress

M3 begins the firmware capability-pack layer accepted by ADR-0007. This milestone does not add a
hardware write, a driver, a transport, a command table or a signed-pack distribution system.
Its first contract is deliberately narrower: descriptive firmware knowledge can be represented,
validated and matched against already-observed identity facts while remaining unable to grant
write authority.

## Starting point

M2 is now merged to `main`. The production Web/PWA path can perform the Rust-owned read-only
identity sequence for the currently accepted API profile, and the exact integrated tree passed
canonical CI and Android development-validation packaging. Physical evidence remains bounded:

- `PHYSICAL_FC_TEST_ATTEMPTED=YES`
- `PHYSICAL_USB_SELECTION_OBSERVED=YES`
- `PHYSICAL_API_SCOPE_GATE_OBSERVED=YES`
- `UNSUPPORTED_API_OUTCOME_OBSERVED=YES`
- `READONLY_IDENTITY_COMPLETION=NO`
- `HARDWARE_SUPPORT_VALIDATED=NO`

M3 does not reinterpret or inflate those facts.

## Slice 1 — descriptive schema and fail-closed resolver

`ade-capability` now models:

- internal firmware family;
- MSP protocol/API range;
- firmware-version range;
- exact target selector only — no wildcard target in this first slice;
- schema version, pack revision and revocation identifier;
- an explicit `ReviewOnlyEmbedded` trust state;
- an explicit `WritesBlocked` policy with no write-enabled variant;
- a minimal privacy-bounded observed identity view;
- a resolver that validates every descriptor before matching and refuses both malformed and
  ambiguous knowledge.

The first embedded review descriptor describes only the legacy Betaflight 4.5.5 / MSP protocol 0
/ API 1.46 / `SPEEDYBEEF405V4` tuple. It is descriptive upstream knowledge, not a claim that the
owner's physical hardware is that target and not a hardware-support claim.

## Safety properties

This slice cannot represent:

- MSP/CLI command ids or arbitrary payloads;
- a transport or device handle;
- a write approval;
- SET/SAVE/EEPROM/reboot/motor/arm/DFU/flashing authority;
- a wildcard target match;
- a signed/trusted distribution claim;
- `HARDWARE_SUPPORT_VALIDATED=YES`.

An invalid descriptor is a terminal typed error instead of being skipped. More than one matching
descriptor is `Ambiguous`; the resolver never chooses one implicitly.

## Distribution boundary

ADR-0007 requires distributed capability packs to be signed, checksummed, versioned and
revocable. The M3 slice intentionally stops before that infrastructure. `ReviewOnlyEmbedded`
means exactly that: repository-reviewed descriptive data, unsuitable for external distribution
and unable to authorise a write. Signed-pack key management, revocation feeds and rollback pack
selection remain part of the later Knowledge Platform milestone.

## Next M3 work

1. Integrate the privacy-bounded identity view with the authoritative Rust facts layer without
   moving protocol bytes into the capability crate.
2. Separate **readable identity profiles** from the still-exact M1 write scope so adding a known
   read-only firmware/API profile can never silently broaden write eligibility.
3. Review pinned official provenance for the next read-only identity profile before changing any
   decoder or command sequence.
4. Add capability-pack selection evidence to the Web diagnostic/result model without exposing
   firmware-engine details in the ordinary product UI.
5. Keep all real writes blocked until the later write milestone and a separate owner approval.

No physical operation is required for this slice.
