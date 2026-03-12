use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::Arc;
use tokio::sync::RwLock;

pub mod health;

#[derive(Debug)]
pub struct Backend {
    pub addr: SocketAddr,
    pub active_connections: AtomicUsize,
    pub is_healthy: AtomicBool,
    pub latency_ewma_us: AtomicUsize,
}

impl Backend {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            active_connections: AtomicUsize::new(0),
            is_healthy: AtomicBool::new(true),
            latency_ewma_us: AtomicUsize::new(0),
        }
    }
}

pub type BackendPool = Arc<RwLock<Vec<Arc<Backend>>>>;