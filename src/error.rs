use crate::key::Key;
use crate::rule::RequestBlockedDetails;
#[cfg(feature = "deadpool")]
use deadpool_redis::PoolError;
use redis::RedisError;
use std::borrow::Cow;
use std::fmt::Display;

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ProvideRuleError<'a> {
    pub detail: Option<Cow<'a, str>>,
    pub key: Option<Key<'a>>,
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
        K: Into<Key<'a>>,
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
        K: Into<Key<'a>>,
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

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error<'a> {
    #[error("rule: {0}")]
    ProvideRule(ProvideRuleError<'a>),

    #[error(transparent)]
    Redis(#[from] RedisError),

    #[cfg(feature = "deadpool")]
    #[error(transparent)]
    Deadpool(#[from] PoolError),

    #[error("request blocked for key {} and can be retried after {} second(s)", .0.rule.key, .0.details.retry_after)]
    RateLimit(RequestBlockedDetails<'a>),
}
