//! Generated Rust bindings for the Synapse FlatBuffers schemas.
//!
//! The schema source of truth lives in `fbs/`. Release CI stages this crate
//! under `target/xtask/packages/rust`, generates bindings there with
//! `flatc --rust --rust-module-root-file`, embeds the schema sources and
//! compiled binary schemas (`fbs/`, `bfbs/`), then publishes it to crates.io.

/// Version of the synapse_fbs release this crate was generated from.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
#[allow(warnings)]
pub mod generated;

pub mod types {
    pub use crate::generated::synapse::types::*;
}

pub mod topic {
    pub use crate::generated::synapse::topic::*;
}

pub mod cmd {
    pub use crate::generated::synapse::cmd::*;
}

pub mod schemas;
pub mod topic_catalog;
pub mod topic_decode;
pub mod value_contract;

#[cfg(feature = "mcap")]
pub mod mcap;
#[cfg(feature = "mcap")]
mod mcap_fixed;

pub use generated::synapse;
