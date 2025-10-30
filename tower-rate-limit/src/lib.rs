use redis::Cmd as RedisCmd;
use redis::aio::ConnectionManager;
use std::{borrow::Cow, pin::Pin, sync::Arc};

pub use redis_cell_client::{Cmd, Policy, PolicyBuilder};

pub trait ExtractKey<R> {
    type Error;

    fn extract<'a>(&self, req: &'a R) -> Result<Cow<'a, str>, Self::Error>;
}

#[derive(Clone)]
enum SuccessHandler<RespTy> {
    Noop,
    Handler(Arc<dyn Fn(RespTy) -> RespTy + Send + Sync + 'static>),
}

#[derive(Clone)]
pub struct RateLimitConfig<Ex, EH, RespTy> {
    extractor: Ex,
    policy: Policy,
    error: EH,
    success: SuccessHandler<RespTy>,
}

impl<Ex, EH, RespTy> RateLimitConfig<Ex, EH, RespTy> {
    pub fn new(extractor: Ex, policy: Policy, error_handler: EH) -> Self {
        RateLimitConfig {
            extractor,
            policy,
            error: error_handler,
            success: SuccessHandler::Noop,
        }
    }

    pub fn set_success_handler<H>(mut self, handler: H) -> Self
    where
        H: Fn(RespTy) -> RespTy + Send + Sync + 'static,
    {
        self.success = SuccessHandler::Handler(Arc::new(handler));
        self
    }
}

#[derive(Clone)]
pub struct RateLimit<S, Ex, EH, RespTy> {
    inner: S,
    config: Arc<RateLimitConfig<Ex, EH, RespTy>>,
    connection: ConnectionManager,
}

impl<S, Ex, EH, SH> RateLimit<S, Ex, EH, SH> {
    pub fn new(
        inner: S,
        config: Arc<RateLimitConfig<Ex, EH, SH>>,
        connection: ConnectionManager,
    ) -> Self {
        RateLimit {
            inner,
            config,
            connection,
        }
    }
}

impl<S, Ex, E, EH, RespTy, ReqTy, IntoRespTy> tower::Service<ReqTy> for RateLimit<S, Ex, EH, RespTy>
where
    S: tower::Service<ReqTy, Response = RespTy> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send,
    S::Response: Send,
    Ex: ExtractKey<ReqTy, Error = E> + Clone + Send + Sync + 'static,
    E: Into<S::Response>,
    ReqTy: Send + 'static,
    EH: Fn() -> IntoRespTy + Clone + Send + Sync + 'static,
    IntoRespTy: Into<S::Response>,
    RespTy: 'static,
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
            inner.call(req).await.map(|resp| match &config.success {
                SuccessHandler::Noop => resp,
                SuccessHandler::Handler(h) => h(resp),
            })
        })
    }
}
