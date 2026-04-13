use prometheus::{Encoder, Registry};
use tokio::task::JoinHandle;
use warp::Filter;

/// Start the Prometheus metrics HTTP server
pub fn start_metrics_server(registry: Registry, port: Option<u16>) -> JoinHandle<()> {
    let metrics_route = warp::path!("metrics").and_then(move || {
        let registry = registry.clone();
        async move {
            let mut buffer = Vec::new();
            let encoder = prometheus::TextEncoder::new();
            encoder.encode(&registry.gather(), &mut buffer).unwrap();
            Ok::<_, warp::Rejection>(String::from_utf8(buffer).unwrap())
        }
    });

    let metrics_port = port.unwrap_or_else(|| {
        std::env::var("METRICS_PORT")
            .map(|p| p.parse::<u16>().expect("Invalid METRICS_PORT"))
            .unwrap_or(9091)
    });

    tokio::spawn(warp::serve(metrics_route).run(([0, 0, 0, 0], metrics_port)))
}
