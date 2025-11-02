use redis::RedisError;
use redis_cell_rs::BlockedDetails;
use redis_cell_rs::Error as RedisCellError;
use std::fmt::Display;
use std::{borrow::Cow, sync::Arc};

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ExtractKeyError {
    pub detail: Option<Cow<'static, str>>,
}

impl Display for ExtractKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("failed to extract key")?;
        if let Some(ref detail) = self.detail {
            f.write_str(": ")?;
            f.write_str(detail)?;
        }
        Ok(())
    }
}

impl ExtractKeyError {
    pub fn with_detail(detail: Cow<'static, str>) -> Self {
        ExtractKeyError {
            detail: Some(detail),
        }
    }
}

impl From<String> for ExtractKeyError {
    fn from(value: String) -> Self {
        ExtractKeyError::with_detail(value.into())
    }
}

impl From<&'static str> for ExtractKeyError {
    fn from(value: &'static str) -> Self {
        ExtractKeyError::with_detail(value.into())
    }
}

#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("extraction: {0}")]
    Extract(ExtractKeyError),

    #[error("failed to apply rule: {0}")]
    Rule(String),

    #[error(transparent)]
    RedisCell(RedisCellError),

    #[error(transparent)]
    Redis(Arc<RedisError>),

    #[error("request blocked and can be retied after {} second(s)", .0.retry_after)]
    Throttle(BlockedDetails),
}
