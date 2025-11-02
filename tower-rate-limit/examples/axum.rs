use axum::http::Request;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{AppendHeaders, IntoResponse, Response};
use axum::{Router, body::Body, routing::get};
use http::header::RETRY_AFTER;
use std::collections::HashMap;
use std::time::Duration;
use tower_rate_limit::redis_cell::Policy;
use tower_rate_limit::{
    Error, ExtractKeyError, ExtractRule, RateLimitConfig, RateLimitLayer, Rule,
};

#[derive(Clone)]
struct ApiKeyExtractor;

impl<T> ExtractRule<Request<T>> for ApiKeyExtractor {
    type Error = ExtractKeyError;
    fn extract<'a>(&self, req: &'a Request<T>) -> Result<Option<Rule<'a>>, Self::Error> {
        let key = req
            .headers()
            .get("x-api-key")
            .and_then(|val| val.to_str().ok())
            .map(Into::into)
            .ok_or(ExtractKeyError::with_detail(
                "'x-api-key' header is missing".into(),
            ))?;
        Ok(Some(Rule::new(key, "basic")))
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // launch a contaier with Valkey (with Redis Cell module)
    let (_container, port) = utils::launch_redis_container().await;
    let connection = utils::build_connection_manager(port).await;

    let basic_policy = Policy::builder()
        .burst(0usize)
        .tokens(1usize)
        .period(Duration::from_secs(3))
        .build();
    let policies = HashMap::from([("basic".to_string(), basic_policy)]);

    let config = RateLimitConfig::new(ApiKeyExtractor, policies, |err, _req| {
        match err {
            Error::Throttle(details) => {
                // trace event
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    // https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Retry-After
                    AppendHeaders([(RETRY_AFTER, details.retry_after)]),
                    Body::from("too many requests"),
                )
                    .into_response()
            }
            Error::Extract(err) => (StatusCode::UNAUTHORIZED, err.to_string()).into_response(),
            Error::Rule(err) => {
                tracing::error!(err = %err, "error in rate limit layer");
                (StatusCode::INTERNAL_SERVER_ERROR).into_response()
            }
            Error::RedisCell(err) => {
                tracing::error!(err = %err, "error in rate limit layer");
                (StatusCode::INTERNAL_SERVER_ERROR).into_response()
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
    .on_success(|_details, resp: &mut Response<Body>| {
        let headers = resp.headers_mut();
        headers.insert(
            "x-inserted-by-success-handler",
            HeaderValue::from_static("<3"),
        );
    });

    // build our application with a single route
    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        .layer(RateLimitLayer::new(config, connection));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

mod utils {
    use redis::aio::ConnectionManager;
    use testcontainers::ContainerAsync;
    use testcontainers::core::IntoContainerPort as _;
    use testcontainers::runners::AsyncRunner;
    use testcontainers::{GenericImage, core::WaitFor};

    pub async fn launch_redis_container() -> (ContainerAsync<GenericImage>, u16) {
        let container = GenericImage::new("valkey-cell", "latest")
            .with_exposed_port(6379.tcp())
            .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
            .start()
            .await
            .unwrap();
        let port = container.get_host_port_ipv4(6379).await.unwrap();
        (container, port)
    }

    pub async fn build_connection_manager(port: u16) -> ConnectionManager {
        let client = redis::Client::open(("localhost", port)).unwrap();
        let config = redis::aio::ConnectionManagerConfig::new().set_number_of_retries(1);
        redis::aio::ConnectionManager::new_with_config(client, config)
            .await
            .unwrap()
    }
}
