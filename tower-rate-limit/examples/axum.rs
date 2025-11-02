use axum::http::Request;
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Router, body::Body, routing::get};
use std::borrow::Cow;
use std::time::Duration;
use tower_rate_limit::redis_cell::Policy;
use tower_rate_limit::{Error, ExtractKey, ExtractKeyError, RateLimitConfig, RateLimitLayer};

#[derive(Clone)]
struct IpExtractor;

#[derive(Debug)]
struct AppError(String);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (StatusCode::OK, self.0).into_response()
    }
}

impl From<AppError> for Response<Body> {
    fn from(value: AppError) -> Self {
        value.into_response()
    }
}

impl<T> ExtractKey<Request<T>> for IpExtractor {
    type Error = ExtractKeyError;
    fn extract<'a>(&self, req: &'a Request<T>) -> Result<Cow<'a, str>, Self::Error> {
        req.headers()
            .get("x-api-key")
            .and_then(|val| val.to_str().ok())
            .map(Into::into)
            .ok_or(ExtractKeyError::with_detail(
                "'x-api-key' header is missing".into(),
            ))
    }
}

#[tokio::main]
async fn main() {
    let (_container, port) = utils::launch_redis_container().await;
    let connection = utils::build_connection_manager(port).await;

    let policy = Policy::builder()
        .burst(0usize)
        .tokens(1usize)
        .period(Duration::from_secs(3))
        .build();

    let config = RateLimitConfig::new(IpExtractor, policy, |err, _req| {
        eprintln!("{}", err);
        match err {
            Error::Throttle(_details) => AppError("rate-limited".to_string()),
            Error::Extract(err) => AppError(format!("uanuthoized: {:?}", err.detail)),
            Error::RedisCell(err) => AppError(format!("internal server error: {}", err)),
            Error::Redis(err) => AppError(format!("internal server error: {:?}", err.detail())),
            _ => AppError("internal server error".into()),
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
