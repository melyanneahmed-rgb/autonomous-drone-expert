# Mock/Replay demonstration

This demonstration is non-hardware evidence. Run it only from the reviewed source commit.

```bash
cargo test --workspace
python3 -m unittest discover -s scripts/tests -v
cargo run -p ade-core-api --example m1_mock_demo
```

The example constructs only `MockFc`, `MockTransport` and `InMemoryAudit`; it has no serial,
USB or operating-system transport. It executes the complete happy lifecycle and prints the
structured `M1RunReport`. It exits unsuccessfully unless the terminal is
`CompletedVerified`, the verification state is `MOCK_EXERCISED`, and all three non-hardware
markers are present. Linux and Windows CI both run it.

The full workspace test run executes both the happy lifecycle and all failure scenarios.
The transport suite then drives the same 26 injected error/frame combinations through
`MockTransport` and `ReplayTransport` and compares:

- returned typed error;
- metadata-only outbound audit;
- sent/blocked disposition and ordering.

Time and cancellation tests use `ManualClock` and `CancellationFlag`. The clock advances
only when the test asks it to; no test sleeps or reads wall-clock time.

For the durable journal, tests create an isolated temporary case file, verify the golden
v1 prefix, reopen it, inject a torn tail, reopen and append again, then separately prove
that checksum corruption and an existing create target are rejected.

Expected report markers on every terminal path:

- `NO HARDWARE CONTACTED`
- `NO HARDWARE SUPPORT CLAIM`
- `REQUIRES HARDWARE TEST`

`MOCK_EXERCISED` and `REPLAY_EXERCISED` are simulation labels. They must never be rewritten
as `HARDWARE_OBSERVED`.
