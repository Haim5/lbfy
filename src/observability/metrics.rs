use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server,
};
use lazy_static::lazy_static;
use prometheus::{register_counter, register_gauge, Counter, Encoder, Gauge, TextEncoder};
use std::convert::Infallible;
use std::net::SocketAddr;

lazy_static! {
    pub static ref TOTAL_CONNECTIONS: Counter = register_counter!(
        "lbfy_connections_total",
        "Total number of accepted connections"
    )
    .unwrap();
    pub static ref ACTIVE_CONNECTIONS: Gauge = register_gauge!(
        "lbfy_connections_active",
        "Current number of active connections"
    )
    .unwrap();
}

async fn serve_req(_req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();

    Ok(Response::new(Body::from(buffer)))
}

pub async fn run_metrics_server(addr: SocketAddr) {
    let make_svc =
        make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(serve_req)) });

    tracing::info!("Metrics server listening on http://{}", addr);
    let server = Server::bind(&addr).serve(make_svc);

    if let Err(e) = server.await {
        tracing::error!("Metrics server error: {}", e);
    }
}