use thiserror::Error;

#[derive(Debug, Error)]
pub enum FerroxError {
    #[error("solver returned infeasible")]
    Infeasible,
    #[error("solver returned unbounded")]
    Unbounded,
    #[error("model invalid: {0}")]
    ModelInvalid(String),
    #[error("solver error")]
    SolverError,
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("no pending request")]
    NoPendingRequest,
}

pub type Result<T> = std::result::Result<T, FerroxError>;

#[cfg(test)]
#[allow(clippy::unnecessary_wraps)]
mod tests {
    use super::*;

    #[test]
    fn display_formats_known_variants() {
        assert_eq!(
            FerroxError::Infeasible.to_string(),
            "solver returned infeasible"
        );
        assert_eq!(
            FerroxError::Unbounded.to_string(),
            "solver returned unbounded"
        );
        assert!(
            FerroxError::ModelInvalid("bad".into())
                .to_string()
                .contains("bad")
        );
        assert_eq!(FerroxError::SolverError.to_string(), "solver error");
        assert_eq!(
            FerroxError::NoPendingRequest.to_string(),
            "no pending request"
        );
    }

    #[test]
    fn from_serde_json_error() {
        let bad: serde_json::Error = serde_json::from_str::<i32>("not json").unwrap_err();
        let err: FerroxError = bad.into();
        assert!(matches!(err, FerroxError::Serde(_)));
        assert!(err.to_string().contains("serialization"));
    }

    #[test]
    fn result_alias_resolves() {
        fn ok() -> Result<i32> {
            Ok(7)
        }
        assert_eq!(ok().unwrap(), 7);
    }
}
