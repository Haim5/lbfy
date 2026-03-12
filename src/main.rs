use std::sync::Arc;
use tokio::sync::RwLock;

mod config;
mod listener;
mod proxy;

// Define empty modules to establish the project structure as per plan.md.
// These will be implemented in later phases.
mod app_state;
mod backend;
mod observability;
mod scheduler;
mod shed;

#[tokio::main]
async fn main() {
    observability::tracing::init();
    tracing::info!("Starting TCP load balancer...");

    // Start metrics server on port 9090
    tokio::spawn(observability::metrics::run_metrics_server(
        "0.0.0.0:9090".parse().unwrap(),
    ));

    // 1. Initialize Backend Pool
    let mut backends = Vec::new();
    for addr_str in config::backends() {
        if let Ok(addr) = addr_str.parse() {
            backends.push(Arc::new(backend::Backend::new(addr)));
        } else {
            tracing::error!("Failed to parse backend address: {}", addr_str);
        }
    }
    let pool = Arc::new(RwLock::new(backends));

    // 2. Initialize Scheduler
    let scheduler = Arc::new(scheduler::round_robin::RoundRobin::new());

    // 3. Initialize Load Shedding (e.g., max 1000 connections)
    let shed_controller = Arc::new(shed::Controller::new(1000));

    let state = app_state::AppState { pool, scheduler, shed_controller };

    // 4. Spawn the health checker
    let health_checker_pool = state.pool.clone();
    tokio::spawn(backend::health::run_health_checks(health_checker_pool));

    listener::run(state).await;
}