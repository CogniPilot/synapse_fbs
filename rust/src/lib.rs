//! Generated Rust bindings for the Synapse FlatBuffers schemas.
//!
//! The schema source of truth lives in `fbs/`. Release CI stages this crate
//! under `target/xtask/packages/rust`, generates bindings there with
//! `flatc --rust --rust-module-root-file`, then publishes it to crates.io.

#[allow(warnings)]
pub mod generated;

pub mod topic {
    pub use crate::generated::synapse::topic::*;
}

pub mod log {
    pub use crate::generated::synapse::log::*;
}

pub mod sil {
    pub use crate::generated::synapse::sil::*;
}

pub use generated::synapse;
