use tauri::{AppHandle, Emitter, Runtime};

use super::Job;

/// Events sent to the frontend. Each one carries a full [`Job`] snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobEvent {
    Created,
    /// Status or progress changed while the job is queued or running.
    Progress,
    /// The job is done or was cancelled.
    Finished,
    Failed,
}

impl JobEvent {
    pub fn name(self) -> &'static str {
        match self {
            JobEvent::Created => "job:created",
            JobEvent::Progress => "job:progress",
            JobEvent::Finished => "job:finished",
            JobEvent::Failed => "job:failed",
        }
    }
}

/// Destination of job events. Abstracted so the queue can be tested without Tauri.
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: JobEvent, job: &Job);
}

pub struct TauriEventSink<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> TauriEventSink<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> EventSink for TauriEventSink<R> {
    fn emit(&self, event: JobEvent, job: &Job) {
        if let Err(err) = self.app.emit(event.name(), job) {
            eprintln!("failed to emit {}: {err}", event.name());
        }
    }
}
