//! Security primitives used by the runtime.
//!
//! - `quarantine`: content-type sniffing and untrusted-instruction heuristics
//!   for the `00-raw/_incoming/` drop zone.

pub mod quarantine;
