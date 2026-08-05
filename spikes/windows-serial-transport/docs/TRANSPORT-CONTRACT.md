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
    fn write(&mut self, buf: &[u8]) -> Result<usize, TransportError>;
    fn flush(&mut self) -> Result<(), TransportError>;
    fn close(self) -> Result<(), TransportError>;
}
```

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
can implement it. It belongs to the session layer as: bounded reads, a cancellation flag
checked between reads, and handle drop as the last resort. Putting `cancel()` in this
trait would be a lie.

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
