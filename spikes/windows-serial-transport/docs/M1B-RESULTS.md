# M1B — Windows Serial Hardware Validation: recorded results

**Status: `M1B HARDWARE VALIDATION — PARTIAL PASS, PAUSED BY OWNER`.**
**`NO BACKEND DECISION — REMAINING HARDWARE TESTS REQUIRED`.**

All results below are `HARDWARE_OBSERVED`, executed by the owner on their Windows machine
with the `m1b` runner. They are recorded here verbatim; no backend is selected from them.

- **Hardware:** owner-described **SpeedyBee F405 AIO 40A** (serial source only; never a
  beeper or flashing target).
- **Windows COM device:** `COM4`.
- **Firmware identity:** `UNKNOWN — MSP PROHIBITED IN M1B`.

> The USB descriptor serial string below (`0X8000000`) is **not** a firmware identity and
> is **not** sufficient evidence to authorise any write. Per ADR/identity rules, writes
> remain blocked pending a read-only firmware identity handshake, which M1B never performs.

## 1. Baseline enumeration — board unplugged — `HARDWARE_OBSERVED`

| Backend | Ports |
| --- | --- |
| serialport | 0 |
| serial2 | 0 |

## 2. Enumeration after USB connect — `HARDWARE_OBSERVED`

| Field | Value |
| --- | --- |
| COM | COM4 |
| VID | 0483 |
| PID | 5740 |
| manufacturer | STMicroelectronics |
| product | STMicroelectronics Virtual COM Port (COM4) |
| reported serial (descriptor) | 0X8000000 |

The reported serial is a USB descriptor string only. It is **not** treated as firmware
identity and **not** sufficient to authorise a write.

## 3. Single-open observation (step 2A) — `HARDWARE_OBSERVED`

| Backend | Result |
| --- | --- |
| serialport | 1/1 clean |
| serial2 | 1/1 clean |

No LED reset · no Windows disconnect/reconnect sound · no COM disappearance or change ·
no DFU device · no unexpected behaviour. The step-2A abort rule was therefore **not**
triggered, and the repeated open/close step was permitted to run.

## 4. Repeated open/close (20 cycles) — `HARDWARE_OBSERVED`

| Backend | Clean cycles | Side effects |
| --- | --- | --- |
| serialport | 20/20 | none observed |
| serial2 | 20/20 | none observed |

## 5. PORT_BUSY — `HARDWARE_OBSERVED`

| Scenario | serialport | serial2 |
| --- | --- | --- |
| Cross-process | `PORT_NOT_FOUND` | `PORT_BUSY` |
| In-process | `PORT_NOT_FOUND` | `PORT_BUSY` |

After the holder was released, COM4 opened again successfully.

**Recorded as an error-classification divergence under contention on this system.**
`PORT_NOT_FOUND` here is **not** interpreted as a real disappearance of the port — the
port was present throughout; `serialport` mapped the contended-open failure to a different
classification than `serial2` did. This is exactly the kind of contention behaviour the
hardware phase exists to surface, and it is a point that a future backend decision must
weigh — not resolve here.

## 6. Read-timeout accuracy — target 250 ms, 100 samples — `HARDWARE_OBSERVED`

| Backend | min | median | p95 | max | data_events | data_bytes | other_errors |
| --- | --- | --- | --- | --- | --- | --- | --- |
| serialport | 250.1 ms | 262.4 ms | 264.4 ms | 265.4 ms | 0 | 0 | 0 |
| serial2 | 250.2 ms | 262.5 ms | 264.6 ms | 265.0 ms | 0 | 0 | 0 |

Both honour the 250 ms floor with a small, comparable overshoot. Zero unsolicited data,
consistent with a request/response firmware that is silent without MSP polling.

## 7. USB unplug during read — `HARDWARE_OBSERVED`

| Backend | Classification | Surfaced into slice | After command start |
| --- | --- | --- | --- |
| serialport | `OPERATION_CANCELLED` | 768.2361 ms | 3.7913736 s |
| serial2 | `OPERATION_CANCELLED` | 64.5854 ms | 4.0882594 s |

**Backend speed must NOT be compared from these numbers:** the manual unplug instant was
not machine-recorded, so the "surfaced into slice" figures are not a like-for-like
latency measurement. What is recorded: both surfaced a classified, non-hanging result on
unplug (here mapped to `OPERATION_CANCELLED`), not a hang.

## 8. Clone-handle diagnostic (drop-original-while-clone-reads) — `HARDWARE_OBSERVED`

| Backend | Observation |
| --- | --- |
| serialport | original dropped at t=3 s; clone returned `READ_TIMEOUT` at ~30.0056673 s; **clone read reached its own timeout** |

Classification: **`HARDWARE_OBSERVED — CLONE HANDLE SEMANTICS ONLY`.** This is **not**
same-handle read cancellation, and it is **not** used on its own to select a backend. On
`serialport`, dropping the original handle did **not** end the clone's in-flight read
early; the clone ran to its own 30 s timeout.

## Complete vs pending

**Complete (this session):**

- Baseline enumeration (both backends)
- Enumeration + metadata capture
- Single-open observation, both backends (step-2A clean)
- Repeated open/close 20×, both backends
- PORT_BUSY cross-process and in-process
- Read-timeout accuracy, both backends
- USB unplug during read, both backends
- Clone-handle diagnostic — **serialport only**

**Pending (required before any backend decision):**

- Step 6 clone-handle diagnostic — **serial2**
- Step 7 — `watch` / unplug / same-port replug / different physical USB port / COM
  renumber observation
- Step 8 — process-kill handle release
- A later clean end-to-end re-run of the full M1B protocol for a final report
- Out of scope on this board: USB→COM join end-to-end (manual test 15) and twin-device
  ambiguity (test 16)

## Standing conclusion

`NO BACKEND DECISION`. The observed divergence in `PORT_BUSY` classification and the
pending cancellation/renumber/process-kill evidence are exactly the material a decision
needs. serialport vs serial2 remains open.
