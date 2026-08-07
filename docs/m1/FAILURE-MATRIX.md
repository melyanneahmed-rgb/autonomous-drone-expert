# M1 failure matrix

The executable tests cover the following 26 lifecycle scenarios. Expected outcomes are
typed; none may silently advance a failed command.

| # | Scenario | Required outcome |
|---:|---|---|
| 1 | Happy path | completed and verified |
| 2 | EEPROM timeout | restored or state unknown with evidence |
| 3 | EEPROM no reply | restored or state unknown with evidence |
| 4 | Corrupt SET reply | recovery |
| 5 | Bad-checksum re-read | recovery |
| 6 | Duplicate reply | abort before write |
| 7 | Replay order mismatch | abort before write |
| 8 | Device absent after reboot | state unknown |
| 9 | Different identity after reboot | state unknown; no recovery write |
| 10 | Resume after transient SET | read and reconcile |
| 11 | Resume after SAVE | reboot and verify; never assume |
| 12 | Resume terminal journal | idempotent rebuild |
| 13 | Disconnect during identification | abort before write |
| 14 | Disconnect during snapshot | abort before write |
| 15 | Disconnect during SET | read-first recovery |
| 16 | Disconnect during SAVE | recovery |
| 17 | Disconnect during REBOOT | recovery |
| 18 | Power loss during SAVE | read-first recovery |
| 19 | Port busy | abort before write |
| 20 | Permission denied | abort before write |
| 21 | Missing driver | abort before write |
| 22 | Recovery SET fails | state unknown |
| 23 | Recovery SAVE fails | state unknown |
| 24 | Recovery REBOOT fails | state unknown |
| 25 | Recovery identity mismatch | state unknown; no write |
| 26 | Recovery final read fails | state unknown |

Additional invariants exercise durable journal overflow/corruption/torn-tail behavior,
write-ahead ordering, deterministic deadlines/cancellation, and identical Mock/Replay
error plus audit evidence for a separate 26-case transport parity matrix.
