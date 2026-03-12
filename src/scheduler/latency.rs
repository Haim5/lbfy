use super::Scheduler;
use crate::backend::Backend;
use rand::seq::SliceRandom;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub struct LatencyAwareScheduler;

impl LatencyAwareScheduler {
    pub fn new() -> Self {
        Self
    }
}

impl Scheduler for LatencyAwareScheduler {
    fn select_backend(&self, pool: &[Arc<Backend>]) -> Option<Arc<Backend>> {
        let healthy_backends: Vec<_> = pool
            .iter()
            .filter(|b| b.is_healthy.load(Ordering::Relaxed))
            .collect();

        if healthy_backends.is_empty() {
            return None;
        }

        if healthy_backends.len() == 1 {
            return Some(healthy_backends[0].clone());
        }

        // Power of Two Choices (P2C)
        let mut rng = rand::thread_rng();
        let choices = healthy_backends.choose_multiple(&mut rng, 2).collect::<Vec<_>>();
        
        let b1 = choices[0];
        let b2 = choices[1];

        let l1 = b1.latency_ewma_us.load(Ordering::Relaxed);
        let l2 = b2.latency_ewma_us.load(Ordering::Relaxed);

        if l1 < l2 {
            Some((*b1).clone())
        } else {
            Some((*b2).clone())
        }
    }
}