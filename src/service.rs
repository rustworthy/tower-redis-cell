use crate::config;
use crate::error::Error;
use crate::rule;
use redis::{FromRedisValue, aio::ConnectionLike};
pub use redis_cell_rs as redis_cell;
use redis_cell_rs::Verdict;
use std::{pin::Pin, sync::Arc};

pub struct RateLimit<S, PR, ReqTy, RespTy, IntoRespTy, C> {
    inner: S,
    config: Arc<config::RateLimitConfig<PR, ReqTy, RespTy, IntoRespTy>>,
    connection: C,
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
        RLC: Into<Arc<config::RateLimitConfig<PR, ReqTy, RespTy, IntoRespTy>>>,
    {
        RateLimit {
            inner,
            config: config.into(),
            connection,
        }
    }
}

impl<S, PR, ReqTy, RespTy, IntoRespTy, C> tower::Service<ReqTy>
    for RateLimit<S, PR, ReqTy, RespTy, IntoRespTy, C>
where
    S: tower::Service<ReqTy, Response = RespTy> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send,
    S::Response: Send,
    PR: rule::ProvideRule<ReqTy> + Clone + Send + Sync + 'static,
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
        let mut connection = self.connection.clone();
        let mut inner = self.inner.clone();
        let config = self.config.clone();

        Box::pin(async move {
            let maybe_rule = match config.rule_provider.provide(&req) {
                Ok(rule) => rule,
                Err(e) => {
                    let config::OnError::Sync(ref h) = config.on_error;
                    let resp = h(Error::ProvideRule(e), &req);
                    return Ok(resp.into());
                }
            };
            let rule = match maybe_rule {
                Some(rule) => rule,
                None => {
                    return inner
                        .call(req)
                        .await
                        .map(|mut resp| match &config.on_unruled {
                            config::OnUnruled::Noop => resp,
                            config::OnUnruled::Sync(h) => {
                                h(&mut resp);
                                resp
                            }
                        });
                }
            };
            let policy = rule.policy;
            let cmd = redis_cell::Cmd::new(&rule.key, &policy);

            let redis_response = match connection.req_packed_command(&cmd.into()).await {
                Ok(res) => res,
                Err(redis_err) => {
                    let config::OnError::Sync(ref h) = config.on_error;
                    let handled = h(redis_err.into(), &req);
                    return Ok(handled.into());
                }
            };
            let redis_cell_verdict = match Verdict::from_redis_value(&redis_response) {
                Ok(verdict) => verdict,
                Err(redis_err) => {
                    let config::OnError::Sync(ref h) = config.on_error;
                    let handled = h(Error::Redis(redis_err), &req);
                    return Ok(handled.into());
                }
            };
            match redis_cell_verdict {
                redis_cell::Verdict::Blocked(details) => {
                    let config::OnError::Sync(ref h) = config.on_error;
                    let handled = h(
                        Error::RateLimit(rule::RequestBlockedDetails { rule, details }),
                        &req,
                    );
                    Ok(handled.into())
                }
                redis_cell::Verdict::Allowed(details) => {
                    let policy = rule.policy;
                    let resource = rule.resource;
                    inner
                        .call(req)
                        .await
                        .map(|mut resp| match &config.on_success {
                            config::OnSuccess::Noop => resp,
                            config::OnSuccess::Sync(h) => {
                                let details = rule::RequestAllowedDetails {
                                    details,
                                    policy,
                                    resource,
                                };
                                h(details, &mut resp);
                                resp
                            }
                        })
                }
            }
        })
    }
}

pub struct RateLimitLayer<PR, ReqTy, RespTy, IntoRespTy, C> {
    config: Arc<config::RateLimitConfig<PR, ReqTy, RespTy, IntoRespTy>>,
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

impl<S, PR, ReqTy, RespTy, IntoRespTy, C> tower::Layer<S>
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
        RLC: Into<Arc<config::RateLimitConfig<PR, ReqTy, RespTy, IntoRespTy>>>,
    {
        RateLimitLayer {
            config: config.into(),
            connection,
        }
    }
}

#[cfg(feature = "deadpool")]
#[cfg_attr(docsrs, doc(cfg(feature = "deadpool")))]
pub mod deadpool {
    use crate::config;
    use crate::error::Error;
    use crate::rule;
    use redis::{FromRedisValue, aio::ConnectionLike};
    pub use redis_cell_rs as redis_cell;
    use redis_cell_rs::Verdict;
    use std::{pin::Pin, sync::Arc};

    pub struct RateLimit<S, PR, ReqTy, RespTy, IntoRespTy> {
        inner: S,
        config: Arc<config::RateLimitConfig<PR, ReqTy, RespTy, IntoRespTy>>,
        pool: deadpool_redis::Pool,
    }

    impl<S, PR, ReqTy, RespTy, IntoRespTy> Clone for RateLimit<S, PR, ReqTy, RespTy, IntoRespTy>
    where
        S: Clone,
    {
        fn clone(&self) -> Self {
            Self {
                inner: self.inner.clone(),
                config: Arc::clone(&self.config),
                pool: self.pool.clone(),
            }
        }
    }

    impl<S, PR, ReqTy, RespTy, IntoRespTy> RateLimit<S, PR, ReqTy, RespTy, IntoRespTy> {
        pub fn new<RLC>(inner: S, config: RLC, pool: deadpool_redis::Pool) -> Self
        where
            RLC: Into<Arc<config::RateLimitConfig<PR, ReqTy, RespTy, IntoRespTy>>>,
        {
            RateLimit {
                inner,
                config: config.into(),
                pool,
            }
        }
    }

    impl<S, PR, ReqTy, RespTy, IntoRespTy> tower::Service<ReqTy>
        for RateLimit<S, PR, ReqTy, RespTy, IntoRespTy>
    where
        S: tower::Service<ReqTy, Response = RespTy> + Clone + Send + 'static,
        S::Future: Send + 'static,
        S::Error: Send,
        S::Response: Send,
        PR: rule::ProvideRule<ReqTy> + Clone + Send + Sync + 'static,
        ReqTy: Send + 'static,
        IntoRespTy: Into<RespTy> + 'static,
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
            let pool = self.pool.clone();
            let mut inner = self.inner.clone();
            let config = self.config.clone();

            Box::pin(async move {
                let maybe_rule = match config.rule_provider.provide(&req) {
                    Ok(rule) => rule,
                    Err(e) => {
                        let config::OnError::Sync(ref h) = config.on_error;
                        let resp = h(Error::ProvideRule(e), &req);
                        return Ok(resp.into());
                    }
                };
                let rule = match maybe_rule {
                    Some(rule) => rule,
                    None => {
                        return inner
                            .call(req)
                            .await
                            .map(|mut resp| match &config.on_unruled {
                                config::OnUnruled::Noop => resp,
                                config::OnUnruled::Sync(h) => {
                                    h(&mut resp);
                                    resp
                                }
                            });
                    }
                };
                let policy = rule.policy;
                let cmd = redis_cell::Cmd::new(&rule.key, &policy);

                let mut connection = match pool.get().await {
                    Ok(conn) => conn,
                    Err(deadpool_err) => {
                        let config::OnError::Sync(ref h) = config.on_error;
                        let handled = h(deadpool_err.into(), &req);
                        return Ok(handled.into());
                    }
                };

                let redis_response = match connection.req_packed_command(&cmd.into()).await {
                    Ok(res) => res,
                    Err(redis_err) => {
                        let config::OnError::Sync(ref h) = config.on_error;
                        let handled = h(redis_err.into(), &req);
                        return Ok(handled.into());
                    }
                };
                let redis_cell_verdict = match Verdict::from_redis_value(&redis_response) {
                    Ok(verdict) => verdict,
                    Err(redis_err) => {
                        let config::OnError::Sync(ref h) = config.on_error;
                        let handled = h(Error::Redis(redis_err), &req);
                        return Ok(handled.into());
                    }
                };
                match redis_cell_verdict {
                    redis_cell::Verdict::Blocked(details) => {
                        let config::OnError::Sync(ref h) = config.on_error;
                        let handled = h(
                            Error::RateLimit(rule::RequestBlockedDetails { rule, details }),
                            &req,
                        );
                        Ok(handled.into())
                    }
                    redis_cell::Verdict::Allowed(details) => {
                        let policy = rule.policy;
                        let resource = rule.resource;
                        inner
                            .call(req)
                            .await
                            .map(|mut resp| match &config.on_success {
                                config::OnSuccess::Noop => resp,
                                config::OnSuccess::Sync(h) => {
                                    let details = rule::RequestAllowedDetails {
                                        details,
                                        policy,
                                        resource,
                                    };
                                    h(details, &mut resp);
                                    resp
                                }
                            })
                    }
                }
            })
        }
    }

    pub struct RateLimitLayer<PR, ReqTy, RespTy, IntoRespTy> {
        config: Arc<config::RateLimitConfig<PR, ReqTy, RespTy, IntoRespTy>>,
        pool: deadpool_redis::Pool,
    }

    impl<PR, ReqTy, RespTy, IntoRespTy> Clone for RateLimitLayer<PR, ReqTy, RespTy, IntoRespTy> {
        fn clone(&self) -> Self {
            Self {
                config: Arc::clone(&self.config),
                pool: self.pool.clone(),
            }
        }
    }

    impl<S, PR, ReqTy, RespTy, IntoRespTy> tower::Layer<S>
        for RateLimitLayer<PR, ReqTy, RespTy, IntoRespTy>
    {
        type Service = RateLimit<S, PR, ReqTy, RespTy, IntoRespTy>;
        fn layer(&self, inner: S) -> Self::Service {
            RateLimit::new(inner, Arc::clone(&self.config), self.pool.clone())
        }
    }

    impl<PR, ReqTy, RespTy, IntoRespTy> RateLimitLayer<PR, ReqTy, RespTy, IntoRespTy> {
        pub fn new<RLC>(config: RLC, pool: deadpool_redis::Pool) -> Self
        where
            RLC: Into<Arc<config::RateLimitConfig<PR, ReqTy, RespTy, IntoRespTy>>>,
        {
            RateLimitLayer {
                config: config.into(),
                pool,
            }
        }
    }
}
