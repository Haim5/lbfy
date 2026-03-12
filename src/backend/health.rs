use crate::backend::BackendPool;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::timeout;

const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(5);
const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(1);

/// Runs active health checks against all backends in a loop.
pub async fn run_health_checks(pool: BackendPool) {
    loop {
        tokio::time::sleep(HEALTH_CHECK_INTERVAL).await;
        tracing::debug!("Running health checks...");

        let backends = pool.read().await;

        for backend in backends.iter() {
            let start = Instant::now();
            let connect_result = timeout(HEALTH_CHECK_TIMEOUT, TcpStream::connect(backend.addr)).await;
            let is_healthy = matches!(connect_result, Ok(Ok(_)));

            if is_healthy {
                let latency = start.elapsed().as_micros() as usize;
                update_ewma(&backend.latency_ewma_us, latency);
            }

            let old_status = backend.is_healthy.swap(is_healthy, Ordering::Relaxed);

            if old_status != is_healthy {
                let status = if is_healthy { "Healthy" } else { "Unhealthy" };
                tracing::info!("Backend {} status changed to: {}", backend.addr, status);
            }
        }
    }
}

fn update_ewma(current_val: &std::sync::atomic::AtomicUsize, new_sample: usize) {
    // Simple EWMA: new_val = (old_val * 0.8) + (sample * 0.2)
    // We use integer math here.
    let old = current_val.load(Ordering::Relaxed);
    if old == 0 {
        current_val.store(new_sample, Ordering::Relaxed);
    } else {
        // new = old * 8/10 + sample * 2/10
        //     = (old * 8 + sample * 2) / 10
        let new_val = (old * 8 + new_sample * 2) / 10;
        current_val.store(new_val, Ordering::Relaxed);
    }
}