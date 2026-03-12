use crate::backend::BackendPool;
use crate::scheduler::Scheduler;
use crate::shed::Controller;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: BackendPool,
    pub scheduler: Arc<dyn Scheduler + Send + Sync>,
    pub shed_controller: Arc<Controller>,
}