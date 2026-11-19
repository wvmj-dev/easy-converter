//! Operations. Each `JobRequest` variant is turned into a [`JobSpec`] here, with its
//! parameters validated, and implemented in `ops/<category>.rs`.

use crate::error::AppResult;
use crate::jobs::{JobRequest, JobSpec};

/// Validates a request and builds the job that runs it.
pub fn build_job(request: JobRequest) -> AppResult<JobSpec> {
    match request {}
}
