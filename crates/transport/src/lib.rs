#![forbid(unsafe_code)]

//! # `ade-transport` — structural placeholder
//!
//! **Planned role:** Physical link abstraction: serial ports and USB enumeration behind replaceable traits. Only crate permitted to hold OS-specific code.
//!
//! Nothing is implemented in this crate yet. It exists to fix module boundaries,
//! ownership and review routing before any logic is written. It contains no
//! dependencies, no protocol constants, no hardware access and no I/O.
//!
//! See `docs/foundational/v1.1.md` section 11 (Technical Architecture).
