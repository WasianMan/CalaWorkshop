//! Shared application state and the in-memory job registry.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::Config;

/// Lifecycle states a job can be in. Serialized exactly as the contract specifies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Queued,
    Downloading,
    Ready,
    Failed,
}

/// A single workshop-download job. This is the object returned by `GET /jobs/{id}`.
///
/// Field names and `null`/omission behaviour follow CONTRACT.md exactly:
/// `file_name` and `size` are only present when `state == ready`; `error` is a
/// string only when `state == failed`.
#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: Uuid,
    pub state: JobState,
    pub app_id: u64,
    pub workshop_id: u64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,

    pub file_token: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,

    /// Always serialized (the contract shows `"error": null`).
    pub error: Option<String>,
}

/// Shared, cloneable handle to everything a request handler needs.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    // Helper jobs are intentionally in-memory; the extension persists the
    // user-facing job history and reconciles active helper jobs while available.
    pub jobs: Arc<RwLock<HashMap<Uuid, Job>>>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
            jobs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Insert a freshly-created job into the registry.
    pub async fn insert_job(&self, job: Job) {
        self.jobs.write().await.insert(job.id, job);
    }

    /// Fetch a clone of a job by id, if it exists.
    pub async fn get_job(&self, id: &Uuid) -> Option<Job> {
        self.jobs.read().await.get(id).cloned()
    }

    /// Apply a mutation to a job in-place, if it exists.
    pub async fn update_job<F: FnOnce(&mut Job)>(&self, id: &Uuid, f: F) {
        if let Some(job) = self.jobs.write().await.get_mut(id) {
            f(job);
        }
    }
}
