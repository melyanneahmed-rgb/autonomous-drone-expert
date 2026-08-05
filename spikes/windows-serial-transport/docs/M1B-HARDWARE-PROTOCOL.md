# M1B — Windows Serial Hardware Validation Protocol

**Hardware approved by the owner:** SpeedyBee F405 AIO 40A — mounted in an aircraft,
propellers and motors connected. **Serial source only.** Not a beeper target, not a
flashing target.

**Board identity policy:** the board's name above is the owner's description. The
protocol records only what USB/COM metadata reports. Firmware identity is
`UNKNOWN — MSP PROHIBITED IN M1B`.

## Absolute rules (every step, no exceptions)

1. **LiPo disconnected the entire time. USB is the only power source.**
2. No MSP, no CLI, no bytes of any kind sent to the board. The `m1b` runner is
   read-only by construction (no write path exists in the binary).
3. No configuration change, no reboot, no DFU, no flashing.
4. No Betaflight Configurator, SpeedyBee App, or any other serial tool running in
   parallel — except where a step explicitly says the port must be busy, which is done
   with a second `m1b` instance, never a configurator.
5. No motor testing. Propellers are installed: treat the aircraft as live anyway.
6. Every port-opening command requires `--confirm-usb-only`.
7. One step at a time. Results go back to the reviewer before the next step starts.

**Disclosure:** opening a serial port asserts DTR/RTS line state, exactly as every
serial tool does. That is a control-line state, not data, and does not reboot the FC.

## Setup (Step 0)

On the Windows test machine:

```
git clone https://github.com/melyanneahmed-rgb/autonomous-drone-expert
cd autonomous-drone-expert
git checkout spike/windows-serial-transport
cd spikes/windows-serial-transport
cargo build --bin m1b
```

`rust-toolchain.toml` pins 1.97.1 and rustup installs it automatically. The board stays
**unplugged** during setup.

## Steps

| # | Test | Command(s) | Board state | What to send back |
| --- | --- | --- | --- | --- |
| 1a | Baseline enumeration | `cargo run --bin m1b -- enumerate` | **Unplugged** | Full output |
| 1b | Metadata capture | plug USB, wait 5s, `enumerate` again | Plugged | Full output — VID/PID/serial/strings as reported, or their absence |
| 2 | Repeated open/close | `open-close --port COMx --backend serialport --cycles 20 --confirm-usb-only` then same with `--backend serial2` | Plugged | Both summaries |
| 3 | Real PORT_BUSY | Terminal 1: `hold --port COMx --backend serialport --hold-secs 120 --confirm-usb-only` · Terminal 2: `busy --port COMx --confirm-usb-only` · then Ctrl+C terminal 1 and run `busy` alone | Plugged | Cross-process and in-process classifications |
| 4 | Read-timeout accuracy | `read-timeout --port COMx --backend serialport --timeout-ms 250 --samples 100 --confirm-usb-only` then `--backend serial2`; optionally repeat at `--timeout-ms 50` | Plugged | min/median/p95/max per backend, plus any unsolicited-data events |
| 5 | Unplug during read | `unplug-read --port COMx --backend serialport --confirm-usb-only`, unplug when prompted; replug, wait 5s, repeat with `serial2` | Plugged → unplugged | The surfaced error, its classification and timing per backend |
| 6 | Drop-cancel | `drop-cancel --port COMx --backend serialport --confirm-usb-only` then `serial2` | Plugged | RESULT line + interpretation line per backend — the decisive cancellation evidence |
| 7 | Replug + renumber | `watch`, then: unplug (10s), replug same USB port; later unplug and replug into a **different physical USB port** | Both | All APPEARED/DISAPPEARED/RENAMED/POSSIBLE lines and the COM numbers seen |
| 8 | Process-kill release | Terminal 1: `hold --port COMx --backend serialport --hold-secs 600 --confirm-usb-only` (prints PID) · Terminal 2: `taskkill /PID <pid> /F` then immediately `open-close --port COMx --backend serialport --cycles 3 --confirm-usb-only` | Plugged | Whether the reopen succeeds immediately, and any delay |

Steps run strictly in order. A step may be repeated; it may never be skipped silently.

## Recording

Each result is pasted back verbatim (the `[HW_RUN]` lines carry everything needed).
Findings are then recorded in `docs/REPORT.md` with the label
`HARDWARE_OBSERVED (SpeedyBee F405 AIO 40A, Windows <version>)` — replacing the
corresponding `REQUIRES_WINDOWS_HARDWARE_TEST` entries — in a follow-up commit on this
spike branch only.

## Out of scope for M1B on this board

USB→COM join end-to-end (manual plan test 15) needs the SetupAPI/WMI prototype which is
deliberately unimplemented; twin-device ambiguity (test 16) needs a second same-model
device. Both remain open.
