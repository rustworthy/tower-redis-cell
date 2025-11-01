use redis::Cmd as RedisCmd;
use redis::aio::ConnectionManager;
use std::{borrow::Cow, pin::Pin, sync::Arc};

mod error;

pub use error::{Error, ExtractKeyError};
pub use redis_cell_rs as redis_cell;

use redis_cell::{AllowedDetails, Cmd, Policy, Verdict};

pub trait ExtractKey<R> {
    type Error;

    fn extract<'a>(&self, req: &'a R) -> Result<Cow<'a, str>, Self::Error>;
}

#[derive(Clone)]
enum SuccessHandler<RespTy> {
    Noop,
    Handler(Arc<dyn Fn(AllowedDetails, &mut RespTy) + Send + Sync + 'static>),
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

    pub fn on_success<H>(mut self, handler: H) -> Self
    where
        H: Fn(AllowedDetails, &mut RespTy) + Send + Sync + 'static,
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
    E: Into<ExtractKeyError>,
    ReqTy: Send + 'static,
    EH: Fn(Error, ReqTy) -> IntoRespTy + Clone + Send + Sync + 'static,
    IntoRespTy: Into<RespTy>,
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
                let e = Error::Extract(e.into());
                let resp = (self.config.error)(e, req);
                return Box::pin(std::future::ready(Ok(resp.into())));
            }
        };
        let cmd = Cmd::new(&key, &self.config.policy);
        let cmd = RedisCmd::from(cmd);
        let mut connection = self.connection.clone();
        let mut inner = self.inner.clone();
        let config = self.config.clone();
        Box::pin(async move {
            let redis_response = match connection.send_packed_command(&cmd).await {
                Ok(res) => res,
                Err(redis) => {
                    let handled = (config.error)(Error::Redis(Arc::new(redis)), req);
                    return Ok(handled.into());
                }
            };
            let redis_cell_verdict: Verdict = match redis_response.try_into() {
                Ok(verdict) => verdict,
                Err(message) => {
                    let handled = (config.error)(Error::Protocol(message), req);
                    return Ok(handled.into());
                }
            };
            match redis_cell_verdict {
                Verdict::Blocked(details) => {
                    let handled = (config.error)(Error::Throttle(details), req);
                    Ok(handled.into())
                }
                Verdict::Allowed(details) => {
                    inner.call(req).await.map(|mut resp| match &config.success {
                        SuccessHandler::Noop => resp,
                        SuccessHandler::Handler(h) => {
                            h(details, &mut resp);
                            resp
                        }
                    })
                }
            }
        })
    }
}
