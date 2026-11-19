use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tokio::sync::{watch, Notify};

use super::events::{EventSink, JobEvent};
use super::job::{Job, JobCategory, JobId, JobStatus};
use crate::error::{AppError, AppResult};
use crate::fs_utils::{self, TempSpace};
use crate::settings::Settings;

/// Minimum interval between two progress events of the same job.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);
/// Progress changes smaller than this (in percentage points) are throttled.
const PROGRESS_STEP: f32 = 0.5;

/// A file produced in the job's temporary folder and where it should end up.
/// The final name is numbered if `destination` already exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedOutput {
    pub temp: PathBuf,
    pub destination: PathBuf,
}

pub type RunFuture = Pin<Box<dyn Future<Output = AppResult<Vec<PlannedOutput>>> + Send>>;
pub type Runner = Box<dyn FnOnce(JobContext) -> RunFuture + Send>;

/// Everything the queue needs to run a job.
pub struct JobSpec {
    pub kind: String,
    pub category: JobCategory,
    pub inputs: Vec<PathBuf>,
    pub runner: Runner,
}

/// Cancellation flag shared between the queue and a running job.
#[derive(Debug, Clone)]
struct CancelToken {
    tx: Arc<watch::Sender<bool>>,
}

impl CancelToken {
    fn new() -> Self {
        Self {
            tx: Arc::new(watch::channel(false).0),
        }
    }

    fn cancel(&self) {
        self.tx.send_replace(true);
    }

    fn is_cancelled(&self) -> bool {
        *self.tx.borrow()
    }

    async fn cancelled(&self) {
        let mut rx = self.tx.subscribe();
        // The sender lives in `self`, so `wait_for` only returns once cancelled.
        let _ = rx.wait_for(|cancelled| *cancelled).await;
    }
}

/// Handed to the code that runs a job.
pub struct JobContext {
    pub id: JobId,
    /// Empty folder for intermediate and output files, removed when the job ends.
    pub temp_dir: PathBuf,
    cancel: CancelToken,
    progress: ProgressReporter,
}

impl JobContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// For operations done in Rust: call between steps to stop early when cancelled.
    pub fn check_cancelled(&self) -> AppResult<()> {
        if self.is_cancelled() {
            Err(AppError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Reports progress from 0 to 100. Events are throttled.
    pub fn set_progress(&self, percent: f32) {
        self.progress.report(Some(percent));
    }

    /// Marks the progress as impossible to measure.
    pub fn set_indeterminate(&self) {
        self.progress.report(None);
    }

    /// Reports `done` out of `total` steps (pages, files...).
    pub fn set_steps(&self, done: usize, total: usize) {
        if total > 0 {
            self.set_progress(done as f32 * 100.0 / total as f32);
        }
    }
}

struct ProgressReporter {
    inner: Arc<Inner>,
    id: JobId,
    last: Mutex<Option<(Option<f32>, Instant)>>,
}

impl ProgressReporter {
    fn report(&self, value: Option<f32>) {
        let value = value.map(|v| {
            if v.is_finite() {
                v.clamp(0.0, 100.0)
            } else {
                0.0
            }
        });
        {
            let mut last = lock(&self.last);
            if let Some((prev, at)) = *last {
                let small_change = match (prev, value) {
                    (Some(a), Some(b)) => (a - b).abs() < PROGRESS_STEP,
                    (None, None) => true,
                    _ => false,
                };
                let too_soon = at.elapsed() < PROGRESS_INTERVAL;
                let is_complete = value == Some(100.0);
                if small_change || (too_soon && !is_complete) {
                    return;
                }
            }
            *last = Some((value, Instant::now()));
        }
        let snapshot = {
            let mut state = lock(&self.inner.state);
            match state.jobs.get_mut(&self.id) {
                Some(job) if job.status == JobStatus::Running => {
                    job.progress = value;
                    job.clone()
                }
                _ => return,
            }
        };
        self.inner.sink.emit(JobEvent::Progress, &snapshot);
    }
}

pub type ConfigProvider = Arc<dyn Fn() -> Settings + Send + Sync>;

struct Pending {
    id: JobId,
    category: JobCategory,
    runner: Runner,
}

struct Running {
    category: JobCategory,
    cancel: CancelToken,
}

#[derive(Default)]
struct State {
    next_id: JobId,
    pending: VecDeque<Pending>,
    running: HashMap<JobId, Running>,
    /// Snapshots of jobs not finished yet.
    jobs: HashMap<JobId, Job>,
    /// Files produced by finished jobs; only these can be opened from the UI.
    outputs: HashSet<PathBuf>,
    shutting_down: bool,
}

struct Inner {
    state: Mutex<State>,
    temp: TempSpace,
    sink: Arc<dyn EventSink>,
    config: ConfigProvider,
    idle: Notify,
}

/// Runs jobs in order, respecting the concurrency limits from the settings.
#[derive(Clone)]
pub struct JobQueue {
    inner: Arc<Inner>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding the lock must not stop the whole queue.
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl JobQueue {
    pub fn new(temp: TempSpace, sink: Arc<dyn EventSink>, config: ConfigProvider) -> Self {
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State {
                    next_id: 1,
                    ..State::default()
                }),
                temp,
                sink,
                config,
                idle: Notify::new(),
            }),
        }
    }

    pub fn temp(&self) -> &TempSpace {
        &self.inner.temp
    }

    /// Adds a job to the queue and starts it as soon as limits allow.
    /// Must be called from within a Tokio runtime.
    pub fn enqueue(&self, spec: JobSpec) -> AppResult<JobId> {
        let snapshot = {
            let mut state = lock(&self.inner.state);
            if state.shutting_down {
                return Err(AppError::NotAllowed("the app is closing".into()));
            }
            let id = state.next_id;
            state.next_id += 1;
            let job = Job {
                id,
                kind: spec.kind,
                category: spec.category,
                inputs: spec.inputs,
                outputs: Vec::new(),
                status: JobStatus::Queued,
                progress: None,
                error: None,
            };
            state.jobs.insert(id, job.clone());
            state.pending.push_back(Pending {
                id,
                category: spec.category,
                runner: spec.runner,
            });
            job
        };
        self.inner.sink.emit(JobEvent::Created, &snapshot);
        schedule(&self.inner);
        Ok(snapshot.id)
    }

    /// Cancels a queued or running job. A running job's process is killed.
    pub fn cancel(&self, id: JobId) -> AppResult<()> {
        let removed = {
            let mut state = lock(&self.inner.state);
            if let Some(running) = state.running.get(&id) {
                running.cancel.cancel();
                return Ok(());
            }
            let Some(pos) = state.pending.iter().position(|p| p.id == id) else {
                return Err(AppError::JobNotFound(id));
            };
            state.pending.remove(pos);
            state.jobs.remove(&id)
        };
        if let Some(mut job) = removed {
            job.status = JobStatus::Cancelled;
            self.inner.sink.emit(JobEvent::Finished, &job);
        }
        notify_if_idle(&self.inner);
        Ok(())
    }

    /// Whether a path was produced by a job of this session.
    pub fn is_known_output(&self, path: &Path) -> bool {
        lock(&self.inner.state).outputs.contains(path)
    }

    /// Cancels everything and waits (up to `timeout`) for running jobs to stop.
    /// Called when the app exits, so no child process outlives it.
    pub async fn shutdown(&self, timeout: Duration) {
        let pending: Vec<JobId> = {
            let mut state = lock(&self.inner.state);
            state.shutting_down = true;
            for running in state.running.values() {
                running.cancel.cancel();
            }
            state.pending.iter().map(|p| p.id).collect()
        };
        for id in pending {
            let _ = self.cancel(id);
        }
        let _ = tokio::time::timeout(timeout, self.wait_idle()).await;
    }

    /// Resolves when no job is queued or running.
    pub async fn wait_idle(&self) {
        loop {
            let notified = self.inner.idle.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let state = lock(&self.inner.state);
                if state.running.is_empty() && state.pending.is_empty() {
                    return;
                }
            }
            notified.await;
        }
    }
}

fn notify_if_idle(inner: &Inner) {
    let state = lock(&inner.state);
    if state.running.is_empty() && state.pending.is_empty() {
        inner.idle.notify_waiters();
    }
}

/// Starts pending jobs while the limits allow. Video jobs have their own, lower limit;
/// when it is reached, later jobs of other categories may start first.
fn schedule(inner: &Arc<Inner>) {
    let config = (inner.config)();
    let max_total = config.concurrency.max(1) as usize;
    let max_video = config.video_concurrency.max(1) as usize;

    let mut started = Vec::new();
    {
        let mut state = lock(&inner.state);
        if state.shutting_down {
            return;
        }
        while state.running.len() < max_total {
            let video_running = state
                .running
                .values()
                .filter(|r| r.category == JobCategory::Video)
                .count();
            let Some(pos) = state
                .pending
                .iter()
                .position(|p| p.category != JobCategory::Video || video_running < max_video)
            else {
                break;
            };
            let Some(pending) = state.pending.remove(pos) else {
                break;
            };
            let cancel = CancelToken::new();
            state.running.insert(
                pending.id,
                Running {
                    category: pending.category,
                    cancel: cancel.clone(),
                },
            );
            let Some(job) = state.jobs.get_mut(&pending.id) else {
                state.running.remove(&pending.id);
                continue;
            };
            job.status = JobStatus::Running;
            let snapshot = job.clone();
            started.push((pending, cancel, snapshot));
        }
    }

    for (pending, cancel, snapshot) in started {
        inner.sink.emit(JobEvent::Progress, &snapshot);
        let timeout = config.timeout_for(pending.category);
        tokio::spawn(run_job(Arc::clone(inner), pending, cancel, timeout));
    }
}

async fn run_job(inner: Arc<Inner>, pending: Pending, cancel: CancelToken, timeout: Duration) {
    let id = pending.id;
    let result = execute(&inner, pending, &cancel, timeout).await;

    let temp = inner.temp.clone();
    let _ = tokio::task::spawn_blocking(move || temp.remove_job_dir(id)).await;

    finish(&inner, id, result);
}

async fn execute(
    inner: &Arc<Inner>,
    pending: Pending,
    cancel: &CancelToken,
    timeout: Duration,
) -> AppResult<Vec<PathBuf>> {
    let id = pending.id;
    let temp_dir = inner.temp.create_job_dir(id)?;
    let ctx = JobContext {
        id,
        temp_dir,
        cancel: cancel.clone(),
        progress: ProgressReporter {
            inner: Arc::clone(inner),
            id,
            last: Mutex::new(None),
        },
    };

    // Run in a separate task so a panic becomes an error instead of a stuck job.
    // Aborting the task drops the runner future, which kills its child processes.
    let mut task = tokio::spawn((pending.runner)(ctx));
    let planned = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            task.abort();
            let _ = task.await;
            Err(AppError::Cancelled)
        }
        _ = tokio::time::sleep(timeout) => {
            cancel.cancel();
            task.abort();
            let _ = task.await;
            Err(AppError::Timeout)
        }
        joined = &mut task => match joined {
            Ok(result) => result,
            Err(err) => Err(AppError::Internal(format!("job task failed: {err}"))),
        },
    }?;

    if cancel.is_cancelled() {
        return Err(AppError::Cancelled);
    }

    tokio::task::spawn_blocking(move || finalize_outputs(planned))
        .await
        .map_err(|err| AppError::Internal(err.to_string()))?
}

/// Moves produced files to their destinations without overwriting anything.
fn finalize_outputs(planned: Vec<PlannedOutput>) -> AppResult<Vec<PathBuf>> {
    let mut finals = Vec::with_capacity(planned.len());
    for output in planned {
        let reserved = fs_utils::reserve_unique_path(&output.destination)?;
        fs_utils::move_into_reserved(&output.temp, &reserved)?;
        finals.push(reserved);
    }
    Ok(finals)
}

fn finish(inner: &Arc<Inner>, id: JobId, result: AppResult<Vec<PathBuf>>) {
    let snapshot = {
        let mut state = lock(&inner.state);
        state.running.remove(&id);
        let Some(mut job) = state.jobs.remove(&id) else {
            return;
        };
        match result {
            Ok(outputs) => {
                state.outputs.extend(outputs.iter().cloned());
                job.status = JobStatus::Done;
                job.progress = Some(100.0);
                job.outputs = outputs;
            }
            Err(AppError::Cancelled) => job.status = JobStatus::Cancelled,
            Err(err) => {
                job.status = JobStatus::Failed;
                job.error = Some(err.payload());
            }
        }
        job
    };
    let event = if snapshot.status == JobStatus::Failed {
        JobEvent::Failed
    } else {
        JobEvent::Finished
    };
    inner.sink.emit(event, &snapshot);
    schedule(inner);
    notify_if_idle(inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<(JobEvent, Job)>>,
    }

    impl EventSink for RecordingSink {
        fn emit(&self, event: JobEvent, job: &Job) {
            lock(&self.events).push((event, job.clone()));
        }
    }

    impl RecordingSink {
        fn final_of(&self, id: JobId) -> Option<Job> {
            lock(&self.events)
                .iter()
                .rev()
                .find(|(e, j)| j.id == id && matches!(e, JobEvent::Finished | JobEvent::Failed))
                .map(|(_, j)| j.clone())
        }

        fn max_running(&self, category: Option<JobCategory>) -> usize {
            let mut running = HashSet::new();
            let mut max = 0;
            for (_, job) in lock(&self.events).iter() {
                if category.is_some_and(|c| c != job.category) {
                    continue;
                }
                if job.status == JobStatus::Running {
                    running.insert(job.id);
                } else {
                    running.remove(&job.id);
                }
                max = max.max(running.len());
            }
            max
        }
    }

    struct Harness {
        queue: JobQueue,
        sink: Arc<RecordingSink>,
        base: PathBuf,
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    fn harness(name: &str, settings: Settings) -> Harness {
        let base = std::env::temp_dir().join(format!(
            "easy-converter-queue-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let temp = TempSpace::create(&base).unwrap();
        let sink = Arc::new(RecordingSink::default());
        let queue = JobQueue::new(temp, sink.clone(), Arc::new(move || settings.clone()));
        Harness { queue, sink, base }
    }

    fn spec<F, Fut>(category: JobCategory, run: F) -> JobSpec
    where
        F: FnOnce(JobContext) -> Fut + Send + 'static,
        Fut: Future<Output = AppResult<Vec<PlannedOutput>>> + Send + 'static,
    {
        JobSpec {
            kind: "test".into(),
            category,
            inputs: vec![],
            runner: Box::new(move |ctx| Box::pin(run(ctx))),
        }
    }

    fn sleeping(category: JobCategory, ms: u64) -> JobSpec {
        spec(category, move |_| async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            Ok(vec![])
        })
    }

    #[tokio::test]
    async fn moves_outputs_and_cleans_temp_folder() {
        let h = harness("outputs", Settings::default());
        let dest_dir = h.base.join("dest");
        std::fs::create_dir_all(&dest_dir).unwrap();
        std::fs::write(dest_dir.join("result.txt"), "existing").unwrap();

        let dest = dest_dir.clone();
        let id = h
            .queue
            .enqueue(spec(JobCategory::Image, move |ctx| async move {
                let temp = ctx.temp_dir.join("result.txt");
                std::fs::write(&temp, "new")?;
                ctx.set_progress(50.0);
                Ok(vec![PlannedOutput {
                    temp,
                    destination: dest.join("result.txt"),
                }])
            }))
            .unwrap();
        h.queue.wait_idle().await;

        let job = h.sink.final_of(id).unwrap();
        assert_eq!(job.status, JobStatus::Done);
        let expected = dest_dir.join("result (2).txt");
        assert_eq!(job.outputs, vec![expected.clone()]);
        assert_eq!(std::fs::read_to_string(&expected).unwrap(), "new");
        assert_eq!(
            std::fs::read_to_string(dest_dir.join("result.txt")).unwrap(),
            "existing"
        );
        assert!(h.queue.is_known_output(&expected));
        assert!(!h.queue.temp().job_dir(id).exists());
    }

    #[tokio::test]
    async fn respects_concurrency_limits() {
        let settings = Settings {
            concurrency: 2,
            video_concurrency: 1,
            ..Settings::default()
        };
        let h = harness("limits", settings);
        for _ in 0..3 {
            h.queue.enqueue(sleeping(JobCategory::Video, 30)).unwrap();
        }
        for _ in 0..4 {
            h.queue.enqueue(sleeping(JobCategory::Image, 30)).unwrap();
        }
        h.queue.wait_idle().await;

        assert_eq!(h.sink.max_running(None), 2);
        assert_eq!(h.sink.max_running(Some(JobCategory::Video)), 1);
        for id in 1..=7 {
            assert_eq!(h.sink.final_of(id).unwrap().status, JobStatus::Done);
        }
    }

    #[tokio::test]
    async fn cancels_queued_and_running_jobs() {
        let settings = Settings {
            concurrency: 1,
            ..Settings::default()
        };
        let h = harness("cancel", settings);
        let running = h
            .queue
            .enqueue(sleeping(JobCategory::Image, 10_000))
            .unwrap();
        let queued = h.queue.enqueue(sleeping(JobCategory::Image, 10)).unwrap();

        h.queue.cancel(queued).unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        h.queue.cancel(running).unwrap();
        tokio::time::timeout(Duration::from_secs(2), h.queue.wait_idle())
            .await
            .unwrap();

        assert_eq!(
            h.sink.final_of(queued).unwrap().status,
            JobStatus::Cancelled
        );
        assert_eq!(
            h.sink.final_of(running).unwrap().status,
            JobStatus::Cancelled
        );
        assert!(matches!(
            h.queue.cancel(running),
            Err(AppError::JobNotFound(_))
        ));
    }

    #[tokio::test]
    async fn times_out_long_jobs() {
        let mut settings = Settings::default();
        settings.timeouts.insert(JobCategory::Image, 1);
        let h = harness("timeout", settings);
        let id = h
            .queue
            .enqueue(sleeping(JobCategory::Image, 10_000_000))
            .unwrap();

        // Let the job start, then run the clock forward past the one-minute timeout.
        tokio::time::sleep(Duration::from_millis(20)).await;
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(61)).await;
        tokio::time::resume();
        tokio::time::timeout(Duration::from_secs(2), h.queue.wait_idle())
            .await
            .unwrap();

        let job = h.sink.final_of(id).unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(job.error.unwrap().code, ErrorCode::Timeout);
    }

    #[tokio::test]
    async fn panics_become_failures() {
        let h = harness("panic", Settings::default());
        let id = h
            .queue
            .enqueue(spec(JobCategory::Pdf, |_| async { panic!("boom") }))
            .unwrap();
        h.queue.wait_idle().await;

        let job = h.sink.final_of(id).unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(job.error.unwrap().code, ErrorCode::Internal);
    }

    #[tokio::test]
    async fn failed_jobs_produce_no_output() {
        let h = harness("failure", Settings::default());
        let dest = h.base.join("never.txt");
        let dest_clone = dest.clone();
        let id = h
            .queue
            .enqueue(spec(JobCategory::Image, move |ctx| async move {
                std::fs::write(ctx.temp_dir.join("partial"), "x")?;
                let _ = dest_clone;
                Err(AppError::ProcessFailed("bad input".into()))
            }))
            .unwrap();
        h.queue.wait_idle().await;

        let job = h.sink.final_of(id).unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(!dest.exists());
        assert!(!h.queue.temp().job_dir(id).exists());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn cancelling_kills_the_child_process() {
        use crate::tools::process::run_streaming;

        let h = harness("kill", Settings::default());
        let (pid_tx, pid_rx) = tokio::sync::oneshot::channel::<u32>();
        let id = h
            .queue
            .enqueue(spec(JobCategory::Video, move |_| async move {
                let mut cmd = tokio::process::Command::new("sh");
                // Print our pid, then replace the shell with a long sleep.
                cmd.args(["-c", "echo $$; exec sleep 30"]);
                let mut pid_tx = Some(pid_tx);
                run_streaming(cmd, move |line| {
                    if let (Some(tx), Ok(pid)) = (pid_tx.take(), line.trim().parse()) {
                        let _ = tx.send(pid);
                    }
                })
                .await?;
                Ok(vec![])
            }))
            .unwrap();

        let pid = tokio::time::timeout(Duration::from_secs(5), pid_rx)
            .await
            .unwrap()
            .unwrap();
        h.queue.cancel(id).unwrap();
        h.queue.wait_idle().await;

        let mut alive = true;
        for _ in 0..50 {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
            // Gone, or a zombie waiting to be reaped.
            if stat.is_empty() || stat.split_whitespace().nth(2) == Some("Z") {
                alive = false;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!alive, "child process {pid} still running");
        assert_eq!(h.sink.final_of(id).unwrap().status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn shutdown_cancels_everything() {
        let settings = Settings {
            concurrency: 1,
            ..Settings::default()
        };
        let h = harness("shutdown", settings);
        let a = h
            .queue
            .enqueue(sleeping(JobCategory::Image, 10_000))
            .unwrap();
        let b = h
            .queue
            .enqueue(sleeping(JobCategory::Image, 10_000))
            .unwrap();

        h.queue.shutdown(Duration::from_secs(2)).await;

        assert_eq!(h.sink.final_of(a).unwrap().status, JobStatus::Cancelled);
        assert_eq!(h.sink.final_of(b).unwrap().status, JobStatus::Cancelled);
        assert!(h.queue.enqueue(sleeping(JobCategory::Image, 1)).is_err());
    }
}
