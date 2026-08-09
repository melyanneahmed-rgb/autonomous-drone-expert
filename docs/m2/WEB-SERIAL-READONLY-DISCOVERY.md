# M2 Web Serial read-only discovery

This gate connects the real Rust WASM identification state machine to a narrow production Web
Serial host. It is discovery only; it does not connect the approved UI or enter configuration,
backup, planning, restore, save, reboot, DFU, CLI, telemetry, or hardware-write flows.

## Authority path

```text
explicit user gesture
  -> requestPort() (memory-only selection)
  -> Rust open directive
  -> four Rust-framed read requests in fixed order
  -> browser byte chunks
  -> bounded Rust MSP response accumulator
  -> canonical typed DeviceIdentity + existing scope check
  -> Rust close directive
```

The browser owns only permission and stream mechanics. It cannot select a command or payload,
construct or interpret MSP, create a write approval, or execute a generic transport effect.
The host uses internal 115200 baud and bounded per-read progress. Reader/writer locks are
released before close; permission, cancellation, busy/unavailable port, disconnect, timeout,
malformed response, protocol failure, and cleanup failure all fail closed.

## Current software evidence

The Chrome gate loads the real generated bridge and production host while injecting a
test-only serial implementation. It covers unavailable API, explicit selection cancellation,
the four fragmented valid replies, authority refusal, response correlation, malformed/checksum,
truncation timeout, disconnect, oversized response, cleanup, and a valid out-of-scope identity.
Static fixtures are project-owned test data. Production JavaScript contains no MSP command
table, parser, or fixture.

## Limitations and next gate

- No production React wiring or UI behavior changes.
- No automatic port enumeration/reconnection and no persisted port metadata.
- No BeeperConfig snapshot and no write/save/reboot/restore operation.
- A scope match is `PROPOSED — NOT HARDWARE VALIDATED`.
- Physical FC behavior, board support, driver behavior, and real disconnect/reconnect remain an
  owner-controlled manual test milestone.

Evidence status: `SOFTWARE_EXERCISED`; `REAL_CHROME_EXERCISED` after CI;
`PHYSICAL_FC_NOT_TESTED`; `HARDWARE_OBSERVED = NO`.
