use redis_cell_rs::{AllowedDetails, BlockedDetails, Policy};
use std::borrow::Cow;

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Rule<'a> {
    pub key: Cow<'a, str>,
    pub policy: Policy,
}

impl<'a> Rule<'a> {
    pub fn new<K>(key: K, policy: Policy) -> Self
    where
        K: Into<Cow<'a, str>>,
    {
        Self {
            key: key.into(),
            policy,
        }
    }
}

pub trait ProvideRule<R> {
    type Error;

    fn provide<'a>(&self, req: &'a R) -> Result<Option<Rule<'a>>, Self::Error>;
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RequestBlockedDetails<'a> {
    pub key: &'a str,
    pub policy: Policy,
    pub details: BlockedDetails,
}

#[non_exhaustive]
pub struct RequestAllowedDetails<'a> {
    pub key: &'a str,
    pub policy: Policy,
    pub details: AllowedDetails,
}
