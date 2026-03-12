use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use crate::observability::metrics;

pub struct Controller {
    active_connections: AtomicUsize,
    max_connections: usize,
}

impl Controller {
    pub fn new(max_connections: usize) -> Self {
        Self {
            active_connections: AtomicUsize::new(0),
            max_connections,
        }
    }

    pub fn try_acquire(self: &Arc<Self>) -> Option<ConnectionGuard> {
        let current = self.active_connections.load(Ordering::Relaxed);
        if current >= self.max_connections {
            return None;
        }
        self.active_connections.fetch_add(1, Ordering::Relaxed);
        metrics::ACTIVE_CONNECTIONS.inc();
        Some(ConnectionGuard {
            controller: self.clone(),
        })
    }
}

pub struct ConnectionGuard {
    controller: Arc<Controller>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.controller.active_connections.fetch_sub(1, Ordering::Relaxed);
        metrics::ACTIVE_CONNECTIONS.dec();
    }
}