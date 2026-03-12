use crate::{app_state::AppState, observability::metrics};
use std::sync::atomic::Ordering;
use tokio::io::copy_bidirectional;
use tokio::net::TcpStream;

/// Handles a single client connection, proxying it to the backend.
pub async fn handle_connection(mut client_stream: TcpStream, state: AppState) {
    let client_addr = match client_stream.peer_addr() {
        Ok(addr) => addr.to_string(),
        Err(_) => "unknown".to_string(),
    };
    tracing::info!("Accepted connection from: {}", client_addr);

    // 1. Select a backend using the scheduler
    let backend = {
        let pool = state.pool.read().await;
        state.scheduler.select_backend(&pool)
    };

    let backend = match backend {
        Some(b) => b,
        None => {
            tracing::warn!("No available backends for {}", client_addr);
            return;
        }
    };

    // 2. Establish a connection to the selected backend.
    let backend_stream = TcpStream::connect(backend.addr).await;

    let mut backend_stream = match backend_stream {
        Ok(stream) => {
            tracing::debug!("Successfully connected to backend: {}", backend.addr);
            // Increment active connections
            backend.active_connections.fetch_add(1, Ordering::Relaxed);
            stream
        }
        Err(e) => {
            tracing::error!("Failed to connect to backend {}: {}", backend.addr, e);
            // Passive health check: mark backend as unhealthy on connection failure.
            backend.is_healthy.store(false, Ordering::Relaxed);
            return;
        }
    };

    // 3. Use Tokio's highly optimized `copy_bidirectional` to forward data.
    // This handles all the logic for reading from one socket and writing to the other,
    // including handling backpressure and half-closed connections.
    match copy_bidirectional(&mut client_stream, &mut backend_stream).await {
        Ok((sent, received)) => {
            tracing::info!(
                "Connection with {} closed. Sent {} bytes, received {} bytes.",
                client_addr, sent, received
            );
        }
        Err(e) => {
            tracing::error!("Error during data proxying for {}: {}", client_addr, e);
        }
    };

    // Decrement active connections
    backend.active_connections.fetch_sub(1, Ordering::Relaxed);
}