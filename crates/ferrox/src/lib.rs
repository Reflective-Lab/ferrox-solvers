pub mod domain_types;
pub mod error;
pub mod provenance;
pub(crate) mod serde_util;
pub mod solver_identity;

#[cfg(test)]
pub(crate) mod test_support;

pub mod catalog;

#[cfg(feature = "ortools")]
pub mod cp;
#[cfg(feature = "ortools")]
pub mod formation;
#[cfg(feature = "ortools")]
pub mod lp;
#[cfg(feature = "highs")]
pub mod mip;
#[cfg(feature = "ortools")]
pub mod network_flow;

pub mod jobshop;
pub mod scheduling;
pub mod vrptw;

pub use converge_pack::ExecutionIdentity;
pub use domain_types::*;
pub use error::{FerroxError, Result};
pub use provenance::{FERROX_PROVENANCE, Ferrox};
