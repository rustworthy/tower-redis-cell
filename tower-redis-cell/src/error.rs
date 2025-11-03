use redis::RedisError;
use redis_cell_rs::BlockedDetails;
use redis_cell_rs::Error as RedisCellError;
use redis_cell_rs::Policy;
use std::fmt::Display;
use std::{borrow::Cow, sync::Arc};

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ProvideRuleError {
    pub detail: Option<Cow<'static, str>>,
}

impl Display for ProvideRuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("failed to provide rule")?;
        if let Some(ref detail) = self.detail {
            f.write_str(": ")?;
            f.write_str(detail)?;
        }
        Ok(())
    }
}

impl ProvideRuleError {
    pub fn with_detail(detail: Cow<'static, str>) -> Self {
        ProvideRuleError {
            detail: Some(detail),
        }
    }
}

impl From<String> for ProvideRuleError {
    fn from(value: String) -> Self {
        ProvideRuleError::with_detail(value.into())
    }
}

impl From<&'static str> for ProvideRuleError {
    fn from(value: &'static str) -> Self {
        ProvideRuleError::with_detail(value.into())
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
#[error("request blocked and can be retied after {} second(s)", .details.retry_after)]
pub struct RequestThrottledError {
    pub details: BlockedDetails,
    pub policy: Policy,
}

#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("rule: {0}")]
    Rule(ProvideRuleError),

    #[error(transparent)]
    RedisCell(RedisCellError),

    #[error(transparent)]
    Redis(Arc<RedisError>),

    #[error("rate-limited: {0}")]
    RateLimit(RequestThrottledError),
}
