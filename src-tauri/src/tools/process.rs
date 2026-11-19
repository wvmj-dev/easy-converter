//! Running external tools as child processes.
//!
//! Commands are always built with an argument list, never through a shell. Children are
//! created with `kill_on_drop`, so dropping the future that runs them (on cancel or
//! timeout) terminates the process.

use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};
use tokio::process::Command;

use crate::error::{AppError, AppResult};

/// Keep only the end of stderr in error messages.
const STDERR_TAIL_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone)]
pub struct ProcessOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

/// Applies the options every child process must have.
pub fn harden(cmd: &mut Command) {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: do not flash a console window for each process.
        cmd.creation_flags(0x0800_0000);
    }
}

/// Runs a command and captures its whole output, giving up after `timeout`.
/// Used for short commands such as `--version`.
pub async fn capture(mut cmd: Command, timeout: Duration) -> AppResult<ProcessOutput> {
    harden(&mut cmd);
    let child = cmd
        .spawn()
        .map_err(|err| AppError::ProcessFailed(format!("could not start process: {err}")))?;
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| AppError::Timeout)??;
    Ok(ProcessOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Runs a command, calling `on_line` for each stdout line (used for progress), and fails
/// with the end of stderr when the process exits with an error.
pub async fn run_streaming<F>(mut cmd: Command, mut on_line: F) -> AppResult<ProcessOutput>
where
    F: FnMut(&str),
{
    harden(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|err| AppError::ProcessFailed(format!("could not start process: {err}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Internal("child stdout not captured".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Internal("child stderr not captured".into()))?;

    let stderr_task = tokio::spawn(read_tail(stderr, STDERR_TAIL_BYTES));

    let mut stdout_text = String::new();
    let mut lines = BufReader::new(stdout).lines();
    while let Some(line) = lines.next_line().await? {
        on_line(&line);
        if stdout_text.len() < STDERR_TAIL_BYTES {
            stdout_text.push_str(&line);
            stdout_text.push('\n');
        }
    }

    let status = child.wait().await?;
    let stderr_text = stderr_task
        .await
        .map_err(|err| AppError::Internal(err.to_string()))??;

    if !status.success() {
        let detail = stderr_text.trim();
        return Err(AppError::ProcessFailed(if detail.is_empty() {
            format!("process exited with {status}")
        } else {
            format!("process exited with {status}: {detail}")
        }));
    }

    Ok(ProcessOutput {
        status,
        stdout: stdout_text,
        stderr: stderr_text,
    })
}

/// Reads a stream to the end, keeping only its last `max` bytes.
async fn read_tail<R: AsyncRead + Unpin>(mut reader: R, max: usize) -> AppResult<String> {
    let mut tail: Vec<u8> = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        tail.extend_from_slice(&buf[..n]);
        if tail.len() > max {
            let excess = tail.len() - max;
            tail.drain(..excess);
        }
    }
    Ok(String::from_utf8_lossy(&tail).into_owned())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", script]);
        cmd
    }

    #[tokio::test]
    async fn streams_stdout_lines() {
        let mut lines = Vec::new();
        let out = run_streaming(sh("printf 'a=1\\nb=2\\n'"), |l| lines.push(l.to_owned()))
            .await
            .unwrap();
        assert!(out.status.success());
        assert_eq!(lines, vec!["a=1", "b=2"]);
    }

    #[tokio::test]
    async fn reports_stderr_on_failure() {
        let err = run_streaming(sh("echo boom >&2; exit 3"), |_| {})
            .await
            .unwrap_err();
        match err {
            AppError::ProcessFailed(msg) => assert!(msg.contains("boom"), "{msg}"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn capture_times_out() {
        let err = capture(sh("sleep 5"), Duration::from_millis(100))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Timeout));
    }

    #[tokio::test]
    async fn arguments_are_not_interpreted_by_a_shell() {
        let mut cmd = Command::new("echo");
        cmd.arg("$(echo injected); rm -rf /");
        let out = capture(cmd, Duration::from_secs(5)).await.unwrap();
        assert_eq!(out.stdout.trim(), "$(echo injected); rm -rf /");
    }
}
