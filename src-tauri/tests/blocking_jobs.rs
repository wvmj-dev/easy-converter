//! Cancellation, timeouts and panics for work done on blocking threads.

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::*;
use easy_converter_lib::error::{AppError, ErrorCode};
use easy_converter_lib::jobs::{JobCategory, JobStatus};
use easy_converter_lib::ops::blocking_job;

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_stops_the_blocking_work() {
    let env = Env::new("blocking-cancel");
    let stopped = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stopped);
    let spec = blocking_job("test", JobCategory::Image, vec![], move |ctx| {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(30) {
            if let Err(err) = ctx.check_cancelled() {
                flag.store(true, Ordering::SeqCst);
                return Err(err);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Err(AppError::Internal("was never cancelled".into()))
    });
    let id = env.queue.enqueue(spec).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    env.queue.cancel(id).unwrap();
    env.queue.wait_idle().await;

    assert_eq!(env.final_of(id).status, JobStatus::Cancelled);
    // The worker thread notices the cancellation at its next check.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !stopped.load(Ordering::SeqCst) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(stopped.load(Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_panic_becomes_a_failed_job() {
    let env = Env::new("blocking-panic");
    let spec = blocking_job("test", JobCategory::Image, vec![], |_ctx| {
        panic!("decoder bug");
    });
    let id = env.queue.enqueue(spec).unwrap();
    env.queue.wait_idle().await;
    assert_failed(&env.final_of(id), ErrorCode::Internal);
}
