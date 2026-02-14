//! API types for rpglot-web JSON serialization.
//!
//! These structures represent the full atomic snapshot sent to web clients.
//! All interned strings are resolved, rates are pre-computed by the server.
//! Clients use the companion schema to interpret units, formats, and views.

pub mod convert;
pub mod schema;
pub mod snapshot;
