use derive_builder::Builder;
use redis::Cmd;
use std::time::Duration;

#[derive(Clone, Debug, Builder)]
#[builder(
    pattern = "mutable",
    derive(Debug),
    custom_constructor,
    setter(into),
    build_fn(name = "try_build", private)
)]
pub struct RedisCellCommand<'a> {
    #[builder(setter(custom))]
    pub key: &'a str,

    #[builder(default = "15")]
    pub burst: usize,

    #[builder(default = "30")]
    pub tokens: usize,

    #[builder(default = "Duration::from_secs(60)")]
    pub period: Duration,

    #[builder(default = "1")]
    pub apply: usize,
}

impl<'a> RedisCellCommand<'a> {
    pub fn builder(key: &'a str) -> RedisCellCommandBuilder<'a> {
        RedisCellCommandBuilder::new(key)
    }
}

impl<'a> RedisCellCommandBuilder<'a> {
    pub fn new(key: &'a str) -> RedisCellCommandBuilder<'a> {
        RedisCellCommandBuilder {
            key: Some(key),
            ..RedisCellCommandBuilder::create_empty()
        }
    }

    pub fn build(&mut self) -> RedisCellCommand<'_> {
        self.try_build().expect("all set")
    }
}

impl From<RedisCellCommand<'_>> for Cmd {
    fn from(val: RedisCellCommand<'_>) -> Self {
        let mut cmd = Cmd::new();
        cmd.arg("CL.THROTTLE")
            .arg(val.key)
            .arg(val.burst)
            .arg(val.tokens)
            .arg(val.period.as_secs())
            .arg(val.apply);
        cmd
    }
}
