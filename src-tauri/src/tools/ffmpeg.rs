//! FFmpeg helpers.

use std::time::Duration;

/// One meaningful line of `ffmpeg -progress pipe:1` output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProgressUpdate {
    /// Position already written to the output.
    Time(Duration),
    /// FFmpeg finished writing.
    End,
}

/// Parses a `key=value` line from `-progress pipe:1`.
///
/// Both `out_time_us` and `out_time_ms` are in microseconds (the latter is misnamed in FFmpeg).
pub fn parse_progress_line(line: &str) -> Option<ProgressUpdate> {
    let (key, value) = line.trim().split_once('=')?;
    match key {
        "out_time_us" | "out_time_ms" => value
            .trim()
            .parse::<u64>()
            .ok()
            .map(|us| ProgressUpdate::Time(Duration::from_micros(us))),
        "progress" if value.trim() == "end" => Some(ProgressUpdate::End),
        _ => None,
    }
}

/// Percentage of `total` covered by `elapsed`, clamped to 0..=100.
pub fn progress_percent(elapsed: Duration, total: Duration) -> Option<f32> {
    if total.is_zero() {
        return None;
    }
    let ratio = elapsed.as_secs_f64() / total.as_secs_f64();
    Some((ratio * 100.0).clamp(0.0, 100.0) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_lines() {
        assert_eq!(
            parse_progress_line("out_time_us=1500000"),
            Some(ProgressUpdate::Time(Duration::from_millis(1500)))
        );
        assert_eq!(
            parse_progress_line("out_time_ms=2000000\n"),
            Some(ProgressUpdate::Time(Duration::from_secs(2)))
        );
        assert_eq!(
            parse_progress_line("progress=end"),
            Some(ProgressUpdate::End)
        );
        assert_eq!(parse_progress_line("progress=continue"), None);
        assert_eq!(parse_progress_line("out_time_us=N/A"), None);
        assert_eq!(parse_progress_line("frame=42"), None);
        assert_eq!(parse_progress_line("garbage"), None);
    }

    #[test]
    fn computes_percent() {
        let total = Duration::from_secs(10);
        assert_eq!(progress_percent(Duration::from_secs(5), total), Some(50.0));
        assert_eq!(
            progress_percent(Duration::from_secs(20), total),
            Some(100.0)
        );
        assert_eq!(
            progress_percent(Duration::from_secs(1), Duration::ZERO),
            None
        );
    }
}
