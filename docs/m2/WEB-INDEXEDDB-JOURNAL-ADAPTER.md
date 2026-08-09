# Browser IndexedDB journal adapter contract

## Authority and scope

This adapter is the browser host for the existing `ade-runtime-ports` storage effects. It
does not create a second journal lifecycle or a JavaScript copy of `EffectJournalStore`.
The Rust core remains authoritative:

```text
EffectJournalStore
  -> IoEffect::Storage
  -> StorageEffect::Load | StorageEffect::CompareAndSwap
  -> browser host adapter
  -> IoResponse::Storage
  -> EffectJournalStore::accept_response
```

The adapter transports complete ADEJ byte values. It does not decode, repair, append to or
reinterpret ADEJ events. In particular, an incomplete final ADEJ record is returned as-is;
the Rust journal owns its narrowly defined repair and emits a new CAS when needed.

## Persistent schema

| Item | Value |
|---|---|
| Database | `autonomous-drone-expert-journal` |
| IndexedDB schema version | `1` |
| Object store | `journals` |
| Key path | `key` |
| Secondary indexes | none |

Each stored object has exactly four properties:

```text
key:           validated 1..=64 lowercase ASCII [a-z0-9_-]
schemaVersion: 1
revision:      canonical unsigned decimal text for the complete u64 domain
bytes:         non-empty ArrayBuffer containing the complete ADEJ value
```

Unknown properties, a wrong record schema, a mismatched key, malformed revision text and
an empty/non-`ArrayBuffer` value are `Corrupt`. No malformed object is automatically
repaired. The database is never deleted or silently reset. Opening a newer database version
or an incompatible store layout fails as `Unavailable`. A `versionchange` closes the stale
connection so a legitimate upgrade is not blocked indefinitely.

## Exact revision model

Storage revisions are `bigint` in JavaScript and canonical decimal text in IndexedDB.
Ordinary JavaScript `number` is never accepted. Text has no sign, whitespace, decimal
point, exponent or leading zero (except the single value `0`) and must be in:

```text
0..=18446744073709551615
```

The first successful create returns revision `1`. A matching update advances exactly once.
Revision `18446744073709551615` cannot advance and fails closed as the existing stable
`Unknown` storage failure. It never wraps to zero and is never reported as success.

## Atomic compare-and-swap

`CompareAndSwap` performs its `get`, validation, comparison and `put` in one IndexedDB
`readwrite` transaction:

| Expected | Stored | Result |
|---|---|---|
| `None` | absent | commit complete bytes at revision `1` |
| `None` | present | `Conflict`, no write |
| `R` | present at `R` | commit complete bytes at `R + 1` |
| `R` | absent | `Conflict`, no write |
| `R` | present at another revision | `Conflict`, no write |

Conflict is never retried and never becomes last-write-wins. A successful `put` request is
only preparation. The adapter reports `Commit` success from the transaction's `complete`
event; transaction error or abort cannot produce success. If JavaScript disappears after
the database committed but before Rust receives the response, the next lifecycle must load
and reconcile the durable bytes/revision. The adapter does not invent a response.

## Stable failure mapping

| Browser/contract condition | Rust storage failure |
|---|---|
| stale/create conflict | `Conflict` |
| `QuotaExceededError` | `QuotaExceeded` |
| `VersionError`, unavailable factory, blocked/incompatible database | `Unavailable` |
| malformed stored record | `Corrupt` |
| `AbortError` not caused by a more specific semantic failure | `Cancelled` |
| revision exhaustion or unrecognised IndexedDB failure | `Unknown` |

Raw DOM exception text, storage keys and journal bytes are never logged or used as product
authority.

## Durability claim

A successful transaction commits the journal into browser-origin IndexedDB and it can
survive normal page reload or browser restart while the browser retains that origin's
storage. This is not an fsync-equivalence claim and does not protect against storage
clearing, origin eviction, profile deletion, disk failure or OS failure. This milestone
does not request persistent-storage permission.

The adapter is not imported by `App.tsx`, `main.tsx` or the service worker. It persists no
UI form state, firmware file, device identifier, user identity, telemetry or remote data.
