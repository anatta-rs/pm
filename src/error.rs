//! Error types — backend-agnostic.

use thiserror::Error;

/// Errors that can occur when talking to an [`crate::IssueTracker`].
#[derive(Debug, Error)]
pub enum PmError {
    /// The argument is malformed or empty.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Authentication failed (missing or wrong token).
    #[error("authentication failed: {0}")]
    Auth(String),

    /// The requested entity does not exist.
    #[error("not found: {0}")]
    NotFound(String),

    /// The backend exceeded its rate limit. Caller should back off.
    #[error("rate limited (retry after {retry_after_seconds}s)")]
    RateLimited {
        /// Suggested wait, in seconds.
        retry_after_seconds: u64,
    },

    /// Network-level failure (DNS, connect, TLS).
    #[error("network error: {0}")]
    Network(String),

    /// The backend's response could not be parsed.
    #[error("parse error: {0}")]
    Parse(String),

    /// Catch-all for backend-specific failures.
    #[error("backend error: {0}")]
    Backend(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, PmError>;

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn invalid_input_displays() {
        let e = PmError::InvalidInput("empty title".into());
        assert_eq!(e.to_string(), "invalid input: empty title");
    }

    #[test]
    fn rate_limited_carries_retry_after() {
        let e = PmError::RateLimited {
            retry_after_seconds: 60,
        };
        assert!(e.to_string().contains("60"));
    }

    #[test]
    fn auth_displays() {
        assert!(
            PmError::Auth("no token".into())
                .to_string()
                .contains("no token")
        );
    }

    #[test]
    fn not_found_displays() {
        assert!(
            PmError::NotFound("issue#42".into())
                .to_string()
                .contains("issue#42")
        );
    }

    #[test]
    fn backend_wraps_inner() {
        let inner = std::io::Error::other("oops");
        let e = PmError::Backend(Box::new(inner));
        assert!(e.to_string().starts_with("backend error"));
        assert!(std::error::Error::source(&e).is_some());
    }

    #[test]
    fn debug_renders() {
        let e = PmError::InvalidInput("x".into());
        assert!(format!("{e:?}").contains("InvalidInput"));
    }

    #[test]
    fn result_alias_works() {
        fn maybe(ok: bool) -> Result<i32> {
            if ok {
                Ok(42)
            } else {
                Err(PmError::InvalidInput("nope".into()))
            }
        }
        assert_eq!(maybe(true).expect("ok"), 42);
        assert!(maybe(false).is_err());
    }
}
