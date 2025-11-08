# tower-redis-cell

[![Crates.io](https://img.shields.io/crates/v/tower-redis-cell.svg)](https://crates.io/crates/tower-redis-cell)
[![Documentation](https://img.shields.io/docsrs/tower-redis-cell/latest)](https://docs.rs/tower-redis-cell/)
[![dependency status](https://deps.rs/repo/github/rustworthy/tower-redis-cell/status.svg)](https://deps.rs/repo/github/rustworthy/tower-redis-cell)

## Description

This crate provides a `Tower` service with rate-limiting functionality
backed by `Valkey` or `Redis` deployed with the [Redis Cell](https://github.com/brandur/redis-cell)
module.

The constucts this crate provides are transport agnostic (just like
[`Tower`](https://github.com/tower-rs/tower) itself), but here is a basic
example using [`axum`](https://github.com/tokio-rs/axum).

First, let's define a `Rule` provider: the rule is defined per request, and
contains `Key` (e.g. IP, API key, user ID), a `Policy`, and - optionally -
a resource name (useful for tracing, debugging, audit).

```rust
use axum::http::Request;
use tower_redis_cell::redis_cell::Policy;
use tower_redis_cell::{ProvideRule, ProvideRuleResult, Rule};

const BASIC_POLICY: Policy = Policy::from_tokens_per_second(1);

#[derive(Clone)]
struct RuleProvider;

impl<T> ProvideRule<Request<T>> for RuleProvider {
    fn provide<'a>(&self, req: &'a Request<T>) -> ProvideRuleResult<'a> {
        let key = req
            .headers()
            .get("x-api-key")
            .and_then(|val| val.to_str().ok())
            .ok_or("cannot define key, since 'x-api-key' header is missing")?;
        Ok(Some(Rule::new(key, BASIC_POLICY)))
  }
}
```

We now need to instantiate `RateLimitConfig` (which expects a rule provider
and an error handler), procure a Valkey/Redis client and use those to create
`RateLimitLayer`. Note that we are using a `ConnectionManager` in this example,
but by default anything [`ConnectionLike`](https://docs.rs/redis/latest/redis/aio/trait.ConnectionLike.html)
will do. There is also an option to use a pool, but you will need to enable
a corresponding feature for that (currently, `deadpool` is supported).

```rust
use axum::http::{StatusCode, header};
use axum::response::{AppendHeaders, IntoResponse, Response};
use axum::{Router, body::Body, routing::get};
use tower_redis_cell::{Error, RateLimitLayer, RateLimitConfig};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let client = redis::Client::open("redis://127.0.0.1/").unwrap();
    let config = redis::aio::ConnectionManagerConfig::new();
    let connection = redis::aio::ConnectionManager::new_with_config(client, config)
        .await
        .unwrap();

    let config = RateLimitConfig::new(RuleProvider, |err, _req| {
        match err {
            Error::ProvideRule(err) => {
                tracing::warn!(
                    key = ?err.key,
                    detail = err.detail.as_deref(),
                    "failed to define rule for request"
                );
                (StatusCode::UNAUTHORIZED, err.to_string()).into_response()
            }
            Error::RateLimit(err) => {
                tracing::warn!(
                    key = %err.rule.key,
                    policy = err.rule.policy.name,
                    "request throttled"
                );
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    AppendHeaders([(header::RETRY_AFTER, err.details.retry_after)]),
                    Body::from("too many requests"),
                )
                    .into_response()
            }
            err => {
                tracing::error!(err = %err, "unexpected error");
                (StatusCode::INTERNAL_SERVER_ERROR).into_response()
            }
        }
    });

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .layer(RateLimitLayer::new(config, connection));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Note that we are in-lining the error handler above, but this can be a free
standing function. Also, you can optionally provide `RateLimitConfig::on_success`
and `RateLimitConfig::on_unruled` handlers, which both provide a mutable access
to the response, and so - if needed - you can set any additional headers.

## Examples

Examples are located [here](./tower-redis-cell/examples). To run an `axum` example,
make sure you got Rust and [docker](https://docs.docker.com/engine/install/) installed:
we are using test containers to launch a `Valkey` server with `Redis Cell` module
loaded.

To run the `axum` example, hit:

```console
cargo run --example axum
```

To run the same example with a `Redis` connection pool:

```console
cargo run --example axum --features deadpool
```

## Development & Contributing

Please find utility commands in [`Makefile`](./Makefile).
