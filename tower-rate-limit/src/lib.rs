use redis::{Cmd as RedisCmd, aio::ConnectionLike};
use std::{borrow::Cow, pin::Pin, sync::Arc};
use tower::Layer;

mod error;

pub use error::{Error, ExtractKeyError};
pub use redis_cell_rs as redis_cell;

use redis_cell::{AllowedDetails, Cmd, Policy, Verdict};

type ReponseEnricher<RespTy> = Arc<dyn Fn(AllowedDetails, &mut RespTy) + Send + Sync + 'static>;

pub trait ExtractKey<R> {
    type Error;

    fn extract<'a>(&self, req: &'a R) -> Result<Cow<'a, str>, Self::Error>;
}

#[derive(Clone)]
enum SuccessHandler<RespTy> {
    Noop,
    Handler(ReponseEnricher<RespTy>),
}

#[derive(Clone)]
pub struct RateLimitConfig<Ex, EH, RespTy> {
    extractor: Ex,
    policy: Policy,
    error_handler: EH,
    success_handler: SuccessHandler<RespTy>,
}

impl<Ex, EH, RespTy> RateLimitConfig<Ex, EH, RespTy> {
    pub fn new(extractor: Ex, policy: Policy, error_handler: EH) -> Self {
        RateLimitConfig {
            extractor,
            policy,
            error_handler,
            success_handler: SuccessHandler::Noop,
        }
    }

    pub fn on_success<H>(mut self, handler: H) -> Self
    where
        H: Fn(AllowedDetails, &mut RespTy) + Send + Sync + 'static,
    {
        self.success_handler = SuccessHandler::Handler(Arc::new(handler));
        self
    }
}

// ############################## SERVICE ####################################
pub struct RateLimit<S, Ex, EH, RespTy, C> {
    inner: S,
    config: Arc<RateLimitConfig<Ex, EH, RespTy>>,
    connection: C, // e.g. `ConnectionManager`
}

impl<S, Ex, EH, RespTy, C> Clone for RateLimit<S, Ex, EH, RespTy, C>
where
    S: Clone,
    C: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            config: Arc::clone(&self.config),
            connection: self.connection.clone(),
        }
    }
}

impl<S, Ex, EH, SH, C> RateLimit<S, Ex, EH, SH, C> {
    pub fn new<RLC>(inner: S, config: RLC, connection: C) -> Self
    where
        RLC: Into<Arc<RateLimitConfig<Ex, EH, SH>>>,
    {
        RateLimit {
            inner,
            config: config.into(),
            connection,
        }
    }
}

impl<S, Ex, E, EH, RespTy, ReqTy, IntoRespTy, C> tower::Service<ReqTy>
    for RateLimit<S, Ex, EH, RespTy, C>
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
    C: ConnectionLike + Clone + Send + 'static,
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
                let resp = (self.config.error_handler)(e, req);
                return Box::pin(std::future::ready(Ok(resp.into())));
            }
        };
        let cmd = Cmd::new(&key, &self.config.policy);
        let cmd = RedisCmd::from(cmd);
        let mut connection = self.connection.clone();
        let mut inner = self.inner.clone();
        let config = self.config.clone();
        Box::pin(async move {
            let redis_response = match connection.req_packed_command(&cmd).await {
                Ok(res) => res,
                Err(redis) => {
                    let handled = (config.error_handler)(Error::Redis(Arc::new(redis)), req);
                    return Ok(handled.into());
                }
            };
            let redis_cell_verdict: Verdict = match redis_response.try_into() {
                Ok(verdict) => verdict,
                Err(message) => {
                    let handled = (config.error_handler)(Error::RedisCell(message), req);
                    return Ok(handled.into());
                }
            };
            match redis_cell_verdict {
                Verdict::Blocked(details) => {
                    let handled = (config.error_handler)(Error::Throttle(details), req);
                    Ok(handled.into())
                }
                Verdict::Allowed(details) => {
                    inner
                        .call(req)
                        .await
                        .map(|mut resp| match &config.success_handler {
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

// ############################## LAYER ######################################
pub struct RateLimitLayer<Ex, EH, RespTy, C> {
    config: Arc<RateLimitConfig<Ex, EH, RespTy>>,
    connection: C,
}

impl<Ex, EH, RespTy, C> Clone for RateLimitLayer<Ex, EH, RespTy, C>
where
    C: Clone,
{
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            connection: self.connection.clone(),
        }
    }
}

impl<S, Ex, EH, RespTy, C> Layer<S> for RateLimitLayer<Ex, EH, RespTy, C>
where
    C: Clone,
{
    type Service = RateLimit<S, Ex, EH, RespTy, C>;
    fn layer(&self, inner: S) -> Self::Service {
        RateLimit::new(inner, Arc::clone(&self.config), self.connection.clone())
    }
}

impl<Ex, EH, RespTy, C> RateLimitLayer<Ex, EH, RespTy, C> {
    pub fn new<RLC>(config: RLC, connection: C) -> Self
    where
        RLC: Into<Arc<RateLimitConfig<Ex, EH, RespTy>>>,
    {
        RateLimitLayer {
            config: config.into(),
            connection,
        }
    }
}
