use redis::Cmd as RedisCmd;
use redis::aio::ConnectionManager;
use std::{borrow::Cow, pin::Pin};

pub trait ExtractKey {
    type Error;
    type Request;

    fn extract<'a>(&self, req: &'a Self::Request) -> Result<Cow<'a, str>, Self::Error>;
}

pub use redis_cell_client::{Cmd, Policy, PolicyBuilder};

#[derive(Clone)]
pub struct RateLimitConfig<Ex> {
    extractor: Ex,
    policy: Policy,
}

impl<Ex> RateLimitConfig<Ex> {
    pub fn new(extractor: Ex, policy: Policy) -> Self {
        RateLimitConfig { extractor, policy }
    }
}

#[derive(Clone)]
pub struct RateLimit<S, Ex> {
    inner: S,
    config: RateLimitConfig<Ex>,
    connection: ConnectionManager,
}

impl<S, Ex> RateLimit<S, Ex> {
    pub fn new(inner: S, config: RateLimitConfig<Ex>, connection: ConnectionManager) -> Self {
        RateLimit {
            inner,
            config,
            connection,
        }
    }
}

impl<S, Ex, E, ReqTy> tower::Service<ReqTy> for RateLimit<S, Ex>
where
    S: tower::Service<ReqTy> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send,
    S::Response: Send,
    Ex: ExtractKey<Request = ReqTy, Error = E>,
    E: Into<S::Response>,
    ReqTy: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: ReqTy) -> Self::Future {
        let key = match self.config.extractor.extract(&req) {
            Ok(key) => key,
            Err(e) => {
                return Box::pin(std::future::ready(Ok(e.into())));
            }
        };
        let cmd = Cmd::new(&key, &self.config.policy);
        let cmd: RedisCmd = cmd.into();

        let mut connection = self.connection.clone();
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let res = connection.send_packed_command(&cmd).await.unwrap();
            let res = res.into_sequence().unwrap();
            let (throttled, total, remaining, restry_after_sesc, reset_after_secs) =
                (&res[0], &res[1], &res[2], &res[3], &res[4]);
            dbg!(&res, &throttled);
            let resp = inner.call(req).await;
            resp
        })
    }
}
