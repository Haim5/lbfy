use super::Scheduler;
use crate::backend::Backend;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct RoundRobin {
    index: AtomicUsize,
}

impl RoundRobin {
    pub fn new() -> Self {
        Self {
            index: AtomicUsize::new(0),
        }
    }
}

impl Scheduler for RoundRobin {
    fn select_backend(&self, pool: &[Arc<Backend>]) -> Option<Arc<Backend>> {
        // Filter for healthy backends first.
        let healthy_backends: Vec<_> = pool
            .iter()
            .filter(|b| b.is_healthy.load(Ordering::Relaxed))
            .cloned()
            .collect();

        if healthy_backends.is_empty() {
            return None;
        }
        let idx = self.index.fetch_add(1, Ordering::Relaxed);
        let backend = &healthy_backends[idx % healthy_backends.len()];
        Some(backend.clone())
    }
}