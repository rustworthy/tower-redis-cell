//use deadpool_redis::redis::{FromRedisValue, cmd};
//use deadpool_redis::{Config, Runtime};
use hyper::header::HeaderValue;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use std::borrow::Cow;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use testcontainers::core::IntoContainerPort as _;
use testcontainers::runners::AsyncRunner;
use testcontainers::{GenericImage, core::WaitFor};
use tower_rate_limit::redis_cell::Policy;
use tower_rate_limit::{Error, ExtractKey, ExtractKeyError, RateLimit, RateLimitConfig};

#[derive(Clone)]
struct IpExtractor;

#[derive(Debug)]
struct AppError(String);

impl From<AppError> for Response<Body> {
    fn from(value: AppError) -> Self {
        Response::new(value.0.into())
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

async fn hello_world(_req: Request<Body>) -> Result<Response<Body>, Infallible> {
    Ok(Response::new("Hello, Tower".into()))
}

#[tokio::main]
async fn main() {
    let container = GenericImage::new("valkey-cell", "latest")
        .with_exposed_port(6379.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(6379).await.unwrap();
    let client = redis::Client::open(("localhost", port)).unwrap();
    let config = redis::aio::ConnectionManagerConfig::new().set_number_of_retries(1);
    let manager = redis::aio::ConnectionManager::new_with_config(client, config)
        .await
        .unwrap();

    //let mut cfg = Config::from_url(format!("redis://localhost:{}", port));
    //let pool = cfg.create_pool(Some(Runtime::Tokio1)).unwrap();
    //let conn = pool.get().await.unwrap();

    // We'll bind to 127.0.0.1:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    let policy = Policy::builder()
        .burst(0usize)
        .tokens(1usize)
        .period(Duration::from_secs(3))
        .build();
    let config = RateLimitConfig::new(IpExtractor, policy, |err, _req| match err {
        Error::Throttle(_details) => AppError("rate-limited".to_string()),
        Error::Extract(err) => AppError(format!("uanuthoized: {:?}", err.detail)),
        Error::Protocol(msg) => AppError(format!("internal server error: {}", msg)),
        Error::Redis(err) => AppError(format!("internal server error: {:?}", err.detail())),
        _ => AppError("internal server error".into()),
    })
    .on_success(|_details, resp: &mut Response<Body>| {
        let headers = resp.headers_mut();
        headers.insert(
            "x-inserted-by-success-handler",
            HeaderValue::from_static("<3"),
        );
    });
    let config = Arc::new(config);

    let svc = make_service_fn(|_conn| {
        let config = Arc::clone(&config);
        let manager = manager.clone();
        async move {
            let svc = service_fn(hello_world);
            let svc = RateLimit::new(svc, config, manager);
            Ok::<_, Infallible>(svc)
        }
    });

    let server = Server::bind(&addr).serve(svc);

    // Run this server for... forever!
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}
