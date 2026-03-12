use crate::{app_state::AppState, config, observability::metrics, proxy};
use tokio::net::TcpListener;

/// Runs the TCP listener and accepts incoming connections.
pub async fn run(state: AppState) {
    let listener = match TcpListener::bind(config::LISTEN_ADDR).await {
        Ok(listener) => {
            tracing::info!("Listening on: {}", config::LISTEN_ADDR);
            listener
        }
        Err(e) => {
            tracing::error!("Failed to bind to address {}: {}", config::LISTEN_ADDR, e);
            return;
        }
    };

    loop {
        match listener.accept().await {
            Ok((client_stream, _)) => {
                // Load Shedding Check
                let _guard = match state.shed_controller.try_acquire() {
                    Some(g) => g,
                    None => {
                        tracing::warn!("Connection rejected due to load shedding");
                        continue;
                    }
                };

                metrics::TOTAL_CONNECTIONS.inc();
                // Spawn a new asynchronous task for each incoming connection.
                // This allows the listener to immediately go back to accepting new connections.
                let state_clone = state.clone();
                // Pass the guard to the handler so it lives as long as the connection
                tokio::spawn(async move {
                    // Move guard into the future
                    let _conn_guard = _guard;
                    proxy::handle_connection(client_stream, state_clone).await;
                });
            }
            Err(e) => {
                tracing::error!("Failed to accept connection: {}", e);
            }
        }
    }
}