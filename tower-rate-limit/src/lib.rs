use redis::Cmd as RedisCmd;
use redis::aio::ConnectionManager;
use std::{borrow::Cow, pin::Pin};

pub use redis_cell_client::{Cmd, Policy, PolicyBuilder};

pub trait ExtractKey<R> {
    type Error;

    fn extract<'a>(&self, req: &'a R) -> Result<Cow<'a, str>, Self::Error>;
}

#[derive(Clone)]
pub struct RateLimitConfig<Ex, EH, SH> {
    extractor: Ex,
    policy: Policy,
    error: EH,
    success: SH,
}

impl<Ex, EH, SH> RateLimitConfig<Ex, EH, SH> {
    pub fn new(extractor: Ex, policy: Policy, error_handler: EH, success_handler: SH) -> Self {
        RateLimitConfig {
            extractor,
            policy,
            error: error_handler,
            success: success_handler,
        }
    }
}

#[derive(Clone)]
pub struct RateLimit<S, Ex, EH, SH> {
    inner: S,
    config: RateLimitConfig<Ex, EH, SH>,
    connection: ConnectionManager,
}

impl<S, Ex, EH, SH> RateLimit<S, Ex, EH, SH> {
    pub fn new(
        inner: S,
        config: RateLimitConfig<Ex, EH, SH>,
        connection: ConnectionManager,
    ) -> Self {
        RateLimit {
            inner,
            config,
            connection,
        }
    }
}

impl<S, Ex, E, EH, SH, ReqTy, RespTy> tower::Service<ReqTy> for RateLimit<S, Ex, EH, SH>
where
    S: tower::Service<ReqTy> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send,
    S::Response: Send,
    Ex: ExtractKey<ReqTy, Error = E> + Clone,
    E: Into<S::Response>,
    ReqTy: Send + 'static,
    EH: Fn() -> RespTy + Clone + Send + 'static,
    RespTy: Into<S::Response>,
    SH: Fn(S::Response) -> S::Response + Clone + Send + 'static,
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
        let config = self.config.clone();
        Box::pin(async move {
            let res = connection.send_packed_command(&cmd).await.unwrap();
            let res = res.into_sequence().unwrap();
            let (throttled, total, remaining, restry_after_sesc, reset_after_secs) =
                (&res[0], &res[1], &res[2], &res[3], &res[4]);
            dbg!(&res, &throttled);
            if *throttled == redis::Value::Int(1) {
                return Ok((config.error)().into());
            }
            match inner.call(req).await {
                Err(e) => Err(e),
                Ok(resp) => Ok((config.success)(resp)),
            }
        })
    }
}
