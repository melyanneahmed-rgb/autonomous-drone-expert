# Smart Configurator — Binding Product Contract

**Accepted:** 2026-08-07
**Scope:** product identity and user/program responsibility boundary

## Product promise

Smart Configurator is an independent, offline-first expert that configures a drone from its
components and intended flight style. Compatibility engines are internal implementation
details, not product identity. The ordinary interface does not display their names or ask the
user to understand their settings.

## The only configuration choices owned by the user

1. Declare component facts that cannot be discovered reliably.
2. Choose the flight intent: Cinematic, Freestyle, Racing or Long Range.
3. Assign physical transmitter switch/button functions manually.
4. Choose trusted online firmware download or a local firmware file.
5. Perform guided physical actions and give approvals required by the safety gates.

The program may observe a switch moving, show its live input and refuse conflicting or unsafe
assignments. It must not choose the switch function on the user's behalf.

## Settings owned automatically by the program

For every domain that is supported by verified capability data, the program derives,
validates, applies and verifies the technical settings, including:

- power, battery and motor/ESC limits;
- ESC protocol and motor behaviour;
- receiver port/protocol and channel order;
- rates, filters, flight control and profile values;
- failsafe and recovery behaviour;
- GPS/navigation, video/OSD, alerts and accessories;
- firmware compatibility, acquisition and the proposed flash plan;
- backup, save, reboot, re-identification, verification and recovery.

Unsupported or unverified domains are refused honestly; they are never pushed back to the
user as unexplained technical decisions.

## Firmware and offline operation

- Trusted download is user-initiated, source-checked and integrity-checked before local use.
- Manual HEX/BIN/DFU selection remains available without Internet and is processed locally.
- Neither path starts flashing automatically.
- Flashing requires exact-target compatibility, a full backup, safe power conditions, a
  declared recovery class and separate explicit approval.
- The installed PWA works offline. Packaged Windows/Android builds later provide first-run
  offline installation and native adapters where browser capabilities are insufficient.

## Safety and honesty

This contract does not authorise a hardware write. Mock/Replay evidence remains simulation
evidence. Real reads, writes, flashing, motors and restore operations keep their existing
approval, verification and recovery gates.
