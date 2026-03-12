pub mod latency;
pub mod round_robin;

use crate::backend::Backend;
use std::sync::Arc;

pub trait Scheduler {
    fn select_backend(&self, pool: &[Arc<Backend>]) -> Option<Arc<Backend>>;
}