use crate::{ProvideRuleError, key::Key};
use redis_cell_rs::{AllowedDetails, BlockedDetails, Policy};

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Rule<'a> {
    pub key: Key<'a>,
    pub policy: Policy,
    pub resource: Option<&'static str>,
}

impl<'a> Rule<'a> {
    pub fn new<K>(key: K, policy: Policy) -> Self
    where
        K: Into<Key<'a>>,
    {
        Self {
            key: key.into(),
            policy,
            resource: None,
        }
    }

    pub fn resource(mut self, resource_name: &'static str) -> Self {
        self.resource = Some(resource_name);
        self
    }
}

pub type ProvideRuleResult<'a> = Result<Option<Rule<'a>>, ProvideRuleError<'a>>;
pub trait ProvideRule<R> {
    fn provide<'a>(&self, req: &'a R) -> ProvideRuleResult<'a>;
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RequestBlockedDetails<'a> {
    pub details: BlockedDetails,
    pub rule: Rule<'a>,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RequestAllowedDetails {
    pub details: AllowedDetails,
    pub policy: Policy,
    pub resource: Option<&'static str>,
}
