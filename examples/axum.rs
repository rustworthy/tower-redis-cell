use axum::http::{Method, Request, StatusCode, header};
use axum::response::{AppendHeaders, IntoResponse, Response};
use axum::routing::post;
use axum::{Router, body::Body, routing::get};
use tower_redis_cell::redis_cell::Policy;
use tower_redis_cell::{Error, ProvideRule, ProvideRuleResult, RateLimitConfig, Rule};

const BASIC_POLICY: Policy = Policy::from_tokens_per_second(1).name("basic");
const STRICT_POLICY: Policy = Policy::from_tokens_per_hour(5).name("strict");

#[derive(Clone)]
struct RuleProvider;

impl<T> ProvideRule<Request<T>> for RuleProvider {
    fn provide<'a>(&self, req: &'a Request<T>) -> ProvideRuleResult<'a> {
        let key = req
            .headers()
            .get("x-api-key")
            .and_then(|val| val.to_str().ok())
            .ok_or("cannot define key, since 'x-api-key' header is missing")?;
        let rule = if req.method() == Method::POST && req.uri().path().contains("/articles") {
            Rule::new(key, STRICT_POLICY).resource("articles::write")
        } else {
            Rule::new(key, BASIC_POLICY)
        };
        Ok(Some(rule))
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // launch a contaier with Valkey (with Redis Cell module)
    let (_container, port) = utils::launch_redis_container().await;

    let rate_limit_config = RateLimitConfig::new(RuleProvider, |err, _req| {
        match err {
            Error::ProvideRule(err) => {
                tracing::warn!(
                    key = ?err.key,
                    detail = err.detail.as_deref(),
                    "failed to defined rule for request"
                );
                (StatusCode::UNAUTHORIZED, err.to_string()).into_response()
            }

            Error::RateLimit(err) => {
                tracing::warn!(
                    key = %err.rule.key,
                    policy = err.rule.policy.name,
                    resource = err.rule.resource,
                    "request throttled"
                );
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Retry-After
                    AppendHeaders([(header::RETRY_AFTER, err.details.retry_after)]),
                    Body::from("too many requests"),
                )
                    .into_response()
            }
            Error::Redis(err) => {
                tracing::error!(err = %err, "error in rate limit layer");
                (StatusCode::INTERNAL_SERVER_ERROR).into_response()
            }
            _ => {
                tracing::error!("unexpected error in rate limit layer");
                (StatusCode::INTERNAL_SERVER_ERROR).into_response()
            }
        }
    })
    // optional success handler
    .on_success(|_details, resp: &mut Response<Body>| {
        let _headers = resp.headers_mut();
        //
    })
    // optional unruled handler (no rule was returned for this request)
    .on_unruled(|_resp: &mut Response<Body>| {
        //
    });

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .route(
            "/articles",
            post(|| async { "will use OpenAI content moderation endpoint" }),
        );

    #[cfg(feature = "deadpool")]
    let app = {
        use deadpool_redis::{Config, Runtime};
        use tower_redis_cell::deadpool::RateLimitLayer;

        let cfg = Config::from_url(format!("redis://localhost:{}", port));
        let pool = cfg.create_pool(Some(Runtime::Tokio1)).unwrap();
        let layer = RateLimitLayer::new(rate_limit_config, pool);
        app.layer(layer)
    };

    #[cfg(not(feature = "deadpool"))]
    let app = {
        use redis::{Client, aio::ConnectionManager, aio::ConnectionManagerConfig};
        use tower_redis_cell::RateLimitLayer;

        let client = Client::open(("localhost", port)).unwrap();
        let cfg = ConnectionManagerConfig::new().set_number_of_retries(1);
        let connection = ConnectionManager::new_with_config(client, cfg)
            .await
            .unwrap();
        let layer = RateLimitLayer::new(rate_limit_config, connection);
        app.layer(layer)
    };

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

mod utils {
    use testcontainers::ContainerAsync;
    use testcontainers::core::IntoContainerPort as _;
    use testcontainers::runners::AsyncRunner;
    use testcontainers::{GenericImage, core::WaitFor};

    pub async fn launch_redis_container() -> (ContainerAsync<GenericImage>, u16) {
        let container = GenericImage::new("ghcr.io/rustworthy/valkey-cell", "latest")
            .with_exposed_port(6379.tcp())
            .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
            .start()
            .await
            .unwrap();
        let port = container.get_host_port_ipv4(6379).await.unwrap();
        (container, port)
    }
}
