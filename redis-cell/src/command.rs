use redis::{Cmd as RedisCmd, ToRedisArgs};
use std::time::Duration;

#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Policy {
    pub burst: usize,

    pub tokens: usize,

    pub period: Duration,

    pub apply: usize,

    pub name: Option<&'static str>,
}

impl Policy {
    pub const fn new(
        burst: usize,
        tokens: usize,
        period: Duration,
        apply: usize,
        name: Option<&'static str>,
    ) -> Policy {
        Self {
            burst,
            tokens,
            period,
            apply,
            name,
        }
    }

    pub const fn from_tokens_per_second(tokens: usize) -> Policy {
        Policy::from_tokens_per_period(tokens, Duration::from_secs(1))
    }

    pub const fn from_tokens_per_minute(tokens: usize) -> Policy {
        Policy::from_tokens_per_period(tokens, Duration::from_secs(60))
    }

    pub const fn from_tokens_per_hour(tokens: usize) -> Policy {
        Policy::from_tokens_per_period(tokens, Duration::from_secs(60 * 60))
    }

    pub const fn from_tokens_per_day(tokens: usize) -> Policy {
        Policy::from_tokens_per_period(tokens, Duration::from_secs(60 * 60 * 24))
    }

    pub const fn from_tokens_per_period(tokens: usize, period: Duration) -> Policy {
        Policy::new(0, tokens, period, 1, None)
    }

    pub const fn max_burst(mut self, burst: usize) -> Policy {
        self.burst = burst;
        self
    }

    pub const fn apply_tokens(mut self, apply: usize) -> Policy {
        self.apply = apply;
        self
    }

    pub const fn name(mut self, name: &'static str) -> Policy {
        self.name = Some(name);
        self
    }
}

pub struct Cmd<'a, K> {
    key: K,
    policy: &'a Policy,
}

impl<'a, K> Cmd<'a, K> {
    pub fn new(key: K, policy: &'a Policy) -> Self {
        Cmd { key, policy }
    }
}

impl<'a, K> From<Cmd<'a, K>> for RedisCmd
where
    K: ToRedisArgs,
{
    fn from(Cmd { key, policy }: Cmd<'a, K>) -> Self {
        let mut cmd = RedisCmd::new();
        cmd.arg("CL.THROTTLE")
            .arg(key)
            .arg(policy.burst)
            .arg(policy.tokens)
            .arg(policy.period.as_secs())
            .arg(policy.apply);
        cmd
    }
}
