use crate::rule::RequestBlockedDetails;
use redis::RedisError;
use redis_cell_rs::Error as RedisCellError;
use std::fmt::Display;
use std::{borrow::Cow, sync::Arc};

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ProvideRuleError<'a> {
    pub key: Option<Cow<'a, str>>,
    pub detail: Option<Cow<'a, str>>,
}

impl Display for ProvideRuleError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("failed to provide rule")?;
        if let Some(ref detail) = self.detail {
            f.write_str(": ")?;
            f.write_str(detail)?;
        }
        Ok(())
    }
}

impl<'a> ProvideRuleError<'a> {
    pub fn new<K, D>(key: K, detail: D) -> Self
    where
        K: Into<Cow<'a, str>>,
        D: Into<Cow<'a, str>>,
    {
        Self::default().key(key).detail(detail)
    }

    pub fn detail<D>(mut self, detail: D) -> Self
    where
        D: Into<Cow<'a, str>>,
    {
        self.detail = Some(detail.into());
        self
    }

    pub fn key<K>(mut self, key: K) -> Self
    where
        K: Into<Cow<'a, str>>,
    {
        self.key = Some(key.into());
        self
    }
}

impl From<String> for ProvideRuleError<'_> {
    fn from(value: String) -> Self {
        ProvideRuleError::default().detail(value)
    }
}

impl<'a> From<&'a str> for ProvideRuleError<'a> {
    fn from(value: &'a str) -> Self {
        ProvideRuleError::default().detail(value)
    }
}

#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error<'a> {
    #[error("rule: {0}")]
    ProvideRule(ProvideRuleError<'a>),

    #[error(transparent)]
    RedisCell(RedisCellError),

    #[error(transparent)]
    Redis(Arc<RedisError>),

    #[error("request blocked for key {} and can be retried after {} second(s)", .0.rule.key, .0.details.retry_after)]
    RateLimit(RequestBlockedDetails<'a>),
}
