pub mod error;
pub(crate) mod serde_util;

#[cfg(test)]
pub(crate) mod test_support;

#[cfg(feature = "ortools")]
pub mod cp;
#[cfg(feature = "ortools")]
pub mod formation;
#[cfg(feature = "ortools")]
pub mod lp;
#[cfg(feature = "highs")]
pub mod mip;

pub mod jobshop;
pub mod scheduling;
pub mod vrptw;

pub use error::{FerroxError, Result};
