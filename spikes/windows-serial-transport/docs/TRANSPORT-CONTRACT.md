# Proposed transport contract (spike proposal — not adopted)

This is a **proposal to be judged**, not an API to be adopted. It lives in the spike
deliberately: `crates/transport` is untouched, and nothing here becomes production
without a separate approved pull request.

## The contract

```rust
pub trait SpikeTransport: Sized {
    fn backend_name() -> &'static str;
    fn metadata_support() -> MetadataSupport;

    fn enumerate() -> Result<Vec<PortInfo>, TransportError>;
    fn open(port_name: &str, config: OpenConfig) -> Result<Self, TransportError>;

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError>;

    // May accept any prefix of the buffer, including zero bytes. Returning is not
    // success.
    fn write_some(&mut self, buf: &[u8]) -> Result<usize, TransportError>;

    fn flush(&mut self) -> Result<(), TransportError>;
    fn close(self) -> Result<(), TransportError>;

    // Drives write_some until every byte is accepted or the deadline expires.
    // Success is declared only at buf.len() bytes — nothing less.
    fn write_all_with_deadline(&mut self, buf: &[u8], deadline: Duration)
        -> Result<usize, WriteAllFailure>;
}
```

**`write_some` versus `write_all_with_deadline`.** A serial write may accept a prefix —
or nothing — and a protocol frame only exists on the wire when *all* of its bytes were
accepted. The contract therefore separates the primitive the OS actually offers
(`write_some`) from the operation the session needs (`write_all_with_deadline`), and the
complete-write loop reports, on failure, exactly how many bytes were accepted
(`WriteAllFailure { bytes_written, error }`) so recovery can reason about a
partially-sent frame. Zero-byte progress is lack of progress, not an error and not
success; the loop keeps trying until its deadline. The loop takes an injected clock in
its testable form, so timeout and disconnect paths are exercised deterministically
without hardware (`tests/write_contract.rs`).

## Decisions and why

**Blocking, not async.** Neither candidate offers cancellation, so an async signature
would promise something the layer cannot deliver. A blocking call on a dedicated thread,
bounded by a timeout and paired with a cancellation flag, is honest about what the
operating system actually provides. An async facade can be added above this later; it
cannot be faked below it.

**`close(self)` consumes.** Both libraries release the handle on drop and neither exposes
an explicit close. Taking `self` makes the release a visible, ordered event in our code
rather than an implicit consequence of scope, which matters when a reconnect follows.

**`metadata_support()` is part of the contract.** A backend that can only report port
names cannot support device identity matching across a reconnect. That is a capability
difference the layer above must be able to see, not a runtime surprise.

**Timeouts are configured at open.** `serialport` folds read and write into a single
`timeout()`; `serial2` sets them separately. The contract carries both, and a backend
that cannot honour the distinction must say so rather than silently apply one to both.

**Cancellation is not in the trait.** It is deliberately absent, because neither candidate
can implement it. The session layer's design is: bounded reads, and a cancellation flag
checked between reads — both `SIMULATED_ONLY` so far. Handle drop is a **candidate**
last-resort mechanism whose behaviour during a real blocked read is
`REQUIRES_WINDOWS_HARDWARE_TEST` and unproven on both libraries; the watchdog in this
spike abandons a hung call, it does not stop it. No candidate holds a proven cancellation
advantage. Putting `cancel()` in this trait would be a lie.

**Disconnect notification is not in the trait either.** Neither library pushes a
disconnect event. Detection is polling enumeration and comparing identity — see
`src/reconnect.rs`. A notification-style API would imply a subscription that does not
exist.

## What still has to be decided before this becomes production

- Whether the session layer owns one thread per port or a shared reactor.
- Whether `read` should return a partial-read indication distinct from a timeout.
- How `OpenConfig` grows for data bits, parity, stop bits and flow control without
  becoming a bag of options.
- Whether enumeration should be cached, and if so with what invalidation.

## Device identity and the reconnect-before-write contract

The identity model (`src/reconnect.rs`) recognises four outcomes:

| Outcome | Meaning |
| --- | --- |
| `UNIQUE_IDENTITY_MATCH` | Non-empty serial numbers present on both sides and equal, same VID/PID |
| `POSSIBLE_MATCH` | Model-level agreement only (VID/PID, optionally manufacturer/product) |
| `AMBIGUOUS_DEVICE_IDENTITY` | More than one live candidate (or colliding serials) — writes blocked until re-identification |
| `NO_MATCH` | Ruled out, or no evidence at all |

Rules: VID/PID/manufacturer/product describe a **model**, never a unit; different
present serials are `NO_MATCH`; a bare COM name is session continuity only and carries no
identity across a disappearance; automatic renames happen only on
`UNIQUE_IDENTITY_MATCH`; look-alikes are surfaced as diagnostics, never acted on.

**Contract (documented only — no MSP in this spike):** OS metadata is never sufficient to
authorise a write after a reconnect. Every resolution — including a unique serial match —
leaves writes blocked until a **read-only firmware identity handshake** re-reads the
board's identity over the protocol and matches it against the session's recorded
identity. Ambiguous resolutions additionally require explicit re-identification before
that handshake. The spike encodes this as `WritePolicy`, which has **no
writes-permitted variant**.
