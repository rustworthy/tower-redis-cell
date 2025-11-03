use redis::{Cmd as RedisCmd, aio::ConnectionLike};
use std::{borrow::Cow, pin::Pin, sync::Arc};
use tower::Layer;

mod error;

pub use error::{Error, ProvideRuleError, RequestThrottledError};
pub use redis_cell_rs as redis_cell;

use redis_cell::{AllowedDetails, Cmd, Policy, Verdict};

type SyncSuccessHandler<RespTy> = Box<dyn Fn(AllowedDetails, &mut RespTy) + Send + Sync + 'static>;

type SyncErrorHandler<ReqTy, IntoRespTy> =
    Box<dyn Fn(Error, ReqTy) -> IntoRespTy + Send + Sync + 'static>;

#[derive(Debug, Clone)]
pub struct Rule<'a> {
    key: Cow<'a, str>,
    policy: Policy,
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

enum OnSuccess<RespTy> {
    Noop,
    Sync(SyncSuccessHandler<RespTy>),
}

enum OnError<ReqTy, IntoRespTy> {
    Sync(SyncErrorHandler<ReqTy, IntoRespTy>),
}

pub struct RateLimitConfig<PR, ReqTy, RespTy, IntoRespTy> {
    rule_provider: PR,
    on_error: OnError<ReqTy, IntoRespTy>,
    on_success: OnSuccess<RespTy>,
}

impl<RP, ReqTy, RespTy, IntoRespTy> RateLimitConfig<RP, ReqTy, RespTy, IntoRespTy> {
    pub fn new<EH>(rule_provider: RP, error_handler: EH) -> Self
    where
        EH: Fn(Error, ReqTy) -> IntoRespTy + Send + Sync + 'static,
    {
        RateLimitConfig {
            rule_provider,
            on_error: OnError::Sync(Box::new(error_handler)),
            on_success: OnSuccess::Noop,
        }
    }

    pub fn on_success<H>(mut self, handler: H) -> Self
    where
        H: Fn(AllowedDetails, &mut RespTy) + Send + Sync + 'static,
    {
        self.on_success = OnSuccess::Sync(Box::new(handler));
        self
    }
}

// ############################## SERVICE ####################################
pub struct RateLimit<S, PR, ReqTy, RespTy, IntoRespTy, C> {
    inner: S,
    config: Arc<RateLimitConfig<PR, ReqTy, RespTy, IntoRespTy>>,
    connection: C, // e.g. `ConnectionManager`
}

impl<S, PR, ReqTy, RespTy, IntoRespTy, C> Clone for RateLimit<S, PR, ReqTy, RespTy, IntoRespTy, C>
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

impl<S, PR, ReqTy, RespTy, IntoRespTy, C> RateLimit<S, PR, ReqTy, RespTy, IntoRespTy, C> {
    pub fn new<RLC>(inner: S, config: RLC, connection: C) -> Self
    where
        RLC: Into<Arc<RateLimitConfig<PR, ReqTy, RespTy, IntoRespTy>>>,
    {
        RateLimit {
            inner,
            config: config.into(),
            connection,
        }
    }
}

impl<S, PR, IntoPRErr, ReqTy, RespTy, IntoRespTy, C> tower::Service<ReqTy>
    for RateLimit<S, PR, ReqTy, RespTy, IntoRespTy, C>
where
    S: tower::Service<ReqTy, Response = RespTy> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send,
    S::Response: Send,
    PR: ProvideRule<ReqTy, Error = IntoPRErr> + Clone + Send + Sync + 'static,
    IntoPRErr: Into<ProvideRuleError>,
    ReqTy: Send + 'static,
    IntoRespTy: Into<RespTy> + 'static,
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
        let maybe_rule = match self.config.rule_provider.provide(&req) {
            Ok(rule) => rule,
            Err(e) => {
                let e = Error::Rule(e.into());
                let OnError::Sync(ref h) = self.config.on_error;
                let resp = h(e, req);
                return Box::pin(std::future::ready(Ok(resp.into())));
            }
        };
        let rule = match maybe_rule {
            Some(rule) => rule,
            None => return Box::pin(self.inner.call(req)),
        };
        let cmd = Cmd::new(&rule.key, &rule.policy);
        let cmd = RedisCmd::from(cmd);

        let mut connection = self.connection.clone();
        let mut inner = self.inner.clone();
        let config = self.config.clone();
        let policy = rule.policy;
        Box::pin(async move {
            let redis_response = match connection.req_packed_command(&cmd).await {
                Ok(res) => res,
                Err(redis) => {
                    let OnError::Sync(ref h) = config.on_error;
                    let handled = h(Error::Redis(Arc::new(redis)), req);
                    return Ok(handled.into());
                }
            };
            let redis_cell_verdict: Verdict = match redis_response.try_into() {
                Ok(verdict) => verdict,
                Err(message) => {
                    let OnError::Sync(ref h) = config.on_error;
                    let handled = h(Error::RedisCell(message), req);
                    return Ok(handled.into());
                }
            };
            match redis_cell_verdict {
                Verdict::Blocked(details) => {
                    let OnError::Sync(ref h) = config.on_error;
                    let handled = h(
                        Error::RateLimit(RequestThrottledError { details, policy }),
                        req,
                    );
                    Ok(handled.into())
                }
                Verdict::Allowed(details) => {
                    inner
                        .call(req)
                        .await
                        .map(|mut resp| match &config.on_success {
                            OnSuccess::Noop => resp,
                            OnSuccess::Sync(h) => {
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
pub struct RateLimitLayer<PR, ReqTy, RespTy, IntoRespTy, C> {
    config: Arc<RateLimitConfig<PR, ReqTy, RespTy, IntoRespTy>>,
    connection: C,
}

impl<PR, ReqTy, RespTy, IntoRespTy, C> Clone for RateLimitLayer<PR, ReqTy, RespTy, IntoRespTy, C>
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

impl<S, PR, ReqTy, RespTy, IntoRespTy, C> Layer<S>
    for RateLimitLayer<PR, ReqTy, RespTy, IntoRespTy, C>
where
    C: Clone,
{
    type Service = RateLimit<S, PR, ReqTy, RespTy, IntoRespTy, C>;
    fn layer(&self, inner: S) -> Self::Service {
        RateLimit::new(inner, Arc::clone(&self.config), self.connection.clone())
    }
}

impl<PR, ReqTy, RespTy, IntoRespTy, C> RateLimitLayer<PR, ReqTy, RespTy, IntoRespTy, C> {
    pub fn new<RLC>(config: RLC, connection: C) -> Self
    where
        RLC: Into<Arc<RateLimitConfig<PR, ReqTy, RespTy, IntoRespTy>>>,
    {
        RateLimitLayer {
            config: config.into(),
            connection,
        }
    }
}
