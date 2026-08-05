# Manual Windows hardware test plan (M1A follow-up)

**Status of every item below: `REQUIRES WINDOWS HARDWARE TEST`.** Nothing here has been
executed. No flight controller has been connected by this project.

## Equipment

- A Windows 10 or 11 x64 machine (physical or VM with real USB passthrough).
- A **disposable** USB-serial device: a CP2102/CH340/FTDI adapter, or a bench flight
  controller that is **not installed in any aircraft that matters**.
- Optionally a second identical adapter, to test that two same-model devices are
  distinguished.
- A powered USB hub, to force COM renumbering.

**Do not use a board mounted in a working drone. Do not connect a battery. Nothing in
this plan sends MSP, writes configuration, or reboots a flight controller.**

## Procedure

| # | Test | Steps | Expected | Evidence to record |
| --- | --- | --- | --- | --- |
| 1 | Enumeration with a device | Plug the adapter, run the harness enumeration test | Port listed with VID, PID and, where the device provides them, manufacturer, product and serial number | Full `PortInfo` for both backends |
| 2 | Metadata gap | Repeat with a device that reports no serial number | Fields absent, no panic, `is_bare()` correct | Output for both backends |
| 3 | Open and close | Open at 115200, close, repeat 20 times | Every cycle succeeds; no handle growth | Handle count before and after |
| 4 | Port busy | Open from process A, then attempt from process B | `PORT_BUSY`, never `PORT_NOT_FOUND` and never an unclassified error | Raw OS code and mapped variant |
| 5 | Read timeout | Open, read with a 250 ms timeout, send nothing | `READ_TIMEOUT` within tolerance | Median and p95 over 100 reads |
| 6 | Write timeout | Open with flow control asserted so writes stall | `WRITE_TIMEOUT` | Observed behaviour per backend |
| 7 | Unplug during read | Start a blocking read, physically unplug | Returns promptly with `DEVICE_DISCONNECTED`, never hangs | Time to return, mapped variant |
| 8 | Cancel during read | Start a blocking read, drop the handle from another thread | Whether the read returns, and how fast | Per backend; this is the decisive cancellation question |
| 9 | Replug, same COM | Replug into the same hub port | Detected as rename or as unchanged, matched by serial number | Delta output |
| 10 | Replug, new COM | Replug into a different hub port to force renumbering | Reported as a **rename**, not as unplug plus plug, when metadata exists | Delta output for both backends |
| 11 | Two identical devices | Plug both, unplug one | The remaining device is never mistaken for the removed one | Match confidence values |
| 12 | Process kill | Kill the process mid-read | The port is immediately openable again; no orphaned handle | Reopen latency after kill |
| 13 | COM above 9 | Force the adapter to COM12 via Device Manager | Opens normally through the `\\.\` device namespace | Success or exact failure |
| 14 | Driver absent | Test with a device whose driver is not installed | A named diagnosis, never a generic failure | Mapped variant |
| 15 | USB → COM join (path C) | Enumerate the adapter with `nusb`, record its instance ID, resolve its COM name via the SetupAPI/registry mechanism, open that COM with `serial2` | The COM name resolved from USB identity matches the port the device actually answers on | Instance ID, resolved name, open result |
| 16 | Twin devices without serials | Two same-model adapters that report no serial number; unplug one, replug it elsewhere | Resolution is `AMBIGUOUS_DEVICE_IDENTITY`; no automatic rename; writes stay blocked until re-identification | Resolution output for both libraries |

## Acceptance for the M1B decision

The architecture choice is not final until tests 4, 5, 7, 8, 10, 12, 15 and 16 have
results on real hardware. Test 8 decides whether any cancellation mechanism exists at
all; test 15 decides whether path C is buildable; test 16 decides how each architecture
degrades when serial numbers are absent — the case the corrected identity model treats
as never-unique.
