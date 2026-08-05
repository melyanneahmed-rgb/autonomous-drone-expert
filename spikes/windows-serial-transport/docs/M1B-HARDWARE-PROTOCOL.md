# M1B — Windows Serial Hardware Validation Protocol (rev 2)

**Hardware approved by the owner:** SpeedyBee F405 AIO 40A — mounted in an aircraft.
**Serial source only.** Not a beeper target, not a flashing target.

**Board identity policy:** the board name above is the owner's description. The protocol
records only what USB/COM metadata reports. Firmware identity is
`UNKNOWN — MSP PROHIBITED IN M1B`.

## Safety gate (mandatory before ANY port-opening command)

1. **LiPo battery disconnected and moved away from the aircraft.**
2. **ALL propellers removed.** Enumeration-only steps (0, 1a, 1b, `watch`) do not open
   the port and may precede removal; **from step 2A onward propeller removal is
   mandatory.**
3. **Betaflight Configurator and SpeedyBee App closed.** Any SpeedyBee phone/Bluetooth
   link disabled if active.
4. **USB is the only connection and the only power source.**
5. **The aircraft is secured on a stable surface.**
6. One step at a time; results go to the reviewer before the next step.

Every port-opening command refuses to run without `--confirm-usb-only`, which attests
this entire gate.

## What the runner is — and is not

The `m1b` binary performs **no payload-byte writes by construction**. It enumerates
ports, opens a port (applying baud/timeout configuration), reads, and closes. It does
not perform `write`, `write_all`, `flush`, purge/discard, MSP, or CLI, and it never
calls any DTR/RTS setter.

**Opening a port is NOT assumed to be side-effect-free.** Opening a serial port may
change driver configuration or control-line state depending on the backend, operating
system and device driver. M1B sends no payload bytes, but any reset, disconnect, LED
change or COM re-enumeration must be recorded.

Control-line facts as currently known:

- `serialport` 4.9.0 is **not assumed** to assert DTR automatically on open; its actual
  behaviour on this machine is `HARDWARE_OBSERVED` material.
- `serial2` and Windows-driver control-line behaviour on open likewise require
  `HARDWARE_OBSERVED` evidence.
- **DTR/RTS are never used as a test command in M1B.**

## Step 0 — setup and source verification (board unplugged)

```
git clone https://github.com/melyanneahmed-rgb/autonomous-drone-expert
cd autonomous-drone-expert
git checkout spike/windows-serial-transport
git fetch origin
git rev-parse HEAD
git rev-parse origin/spike/windows-serial-transport
```

**Both SHAs must be identical, and must equal the corrected-build SHA announced by the
reviewer in the conversation.** Running an older checkout of this branch is not
permitted. Only then:

```
cd spikes\windows-serial-transport
cargo build --bin m1b
```

## Steps (strictly in order; each result reviewed before the next)

| # | Test | Command(s) | Props | What to send back |
| --- | --- | --- | --- | --- |
| 1a | Baseline enumeration (board unplugged) | `cargo run --bin m1b -- enumerate` | may be on | Full output |
| 1b | Metadata capture | plug USB, wait 5s, `enumerate` again | may be on | Full output |
| **2A** | **Single-open observation** | `single-open --port COMx --backend serialport --confirm-usb-only`, then the same with `serial2`. Watch the board and Device Manager during the 3-second dwell | **removed** | Answers to the five printed questions per backend: LED restart? COM disappeared/returned? COM number changed? DFU device appeared? sound/behaviour change? |
| — | **Abort rule** | If 2A shows any unexpected reboot or re-enumeration: **STOP M1B.** Classification: `UNEXPECTED OPEN SIDE EFFECT — INVESTIGATION REQUIRED`. The 20-cycle step must not run. | — | — |
| 2B | Repeated open/close (only after 2A is clean on both backends) | `open-close --port COMx --backend serialport --cycles 20 --confirm-usb-only`, then `serial2` | removed | Both summaries |
| 3 | Real PORT_BUSY | Terminal 1: `hold --port COMx --backend serialport --hold-secs 120 --confirm-usb-only` · Terminal 2: `busy --port COMx --confirm-usb-only` · then Ctrl+C terminal 1 and run `busy` alone | removed | Cross-process and in-process classifications |
| 4 | Read-timeout accuracy | `read-timeout --port COMx --backend serialport --timeout-ms 250 --samples 100 --confirm-usb-only`, then `serial2` | removed | min/median/p95/max per backend; unsolicited-data **counts** (content is never recorded) |
| 5 | Unplug during read | `unplug-read --port COMx --backend serialport --confirm-usb-only`; unplug when prompted; replug, wait 5s, repeat with `serial2` | removed | Surfaced error, classification and timing per backend |
| 6 | Drop-original-while-clone-reads — **diagnostic only** | `drop-original-while-clone-reads --port COMx --backend serialport --confirm-usb-only`, then `serial2` | removed | The RESULT and observation lines per backend |
| 7 | Replug + renumber | `watch`; unplug (10s), replug same USB port; later replug into a **different physical USB port** | removed | All APPEARED/DISAPPEARED/RENAMED/POSSIBLE lines and COM numbers |
| 8 | Process-kill release | Terminal 1: `hold --port COMx --backend serialport --hold-secs 600 --confirm-usb-only` (prints PID) · Terminal 2: `taskkill /PID <pid> /F`, then immediately `open-close --port COMx --backend serialport --cycles 3 --confirm-usb-only` | removed | Whether reopen succeeds immediately, and any delay |

## What step 6 does and does not test

It observes **clone-handle semantics only**: a clone blocks in a read while the original
handle is dropped. The possible observations are exactly:

- `clone read returned early`
- `clone read reached its own timeout`
- `clone read remained stuck`

Its classification is always `HARDWARE_OBSERVED — CLONE HANDLE SEMANTICS ONLY`, it is
**not** a same-handle cancellation test, and its outcome is **not used for backend
selection**. Same-handle thread cancellation remains
`UNRESOLVED — NO PUBLIC SAME-HANDLE CANCELLATION PRIMITIVE PROVEN`. Step 8
(process-kill) is an independent test of process-level handle release, not a substitute
for thread cancellation.

## Recording

Results are pasted back verbatim (`[HW_RUN]` lines). Raw payload content is never
recorded anywhere — only byte counts, timestamps and backend names. Findings then land
in `docs/REPORT.md` as `HARDWARE_OBSERVED (SpeedyBee F405 AIO 40A, Windows <version>)`
in a follow-up commit on this spike branch only.

## Out of scope for M1B on this board

USB→COM join end-to-end (manual plan test 15) and twin-device ambiguity (test 16)
remain open — they need the unimplemented SetupAPI/WMI prototype and a second same-model
device respectively.
