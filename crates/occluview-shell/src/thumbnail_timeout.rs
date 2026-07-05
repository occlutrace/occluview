//! Bounded worker execution for shell thumbnail requests.

use std::sync::mpsc;
use std::time::Duration;

/// Run `work` on a background thread and wait up to `timeout`.
///
/// # Errors
/// This function encodes failures as `None`: timeout, worker panic, thread spawn
/// failure, or channel disconnect all produce no result. The caller decides the
/// fallback behavior.
#[must_use]
pub fn run_with_timeout<T, F>(timeout: Duration, work: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("occluview-thumbnail".to_string())
        .spawn(move || {
            let _ = tx.send(work());
        })
        .ok()?;
    rx.recv_timeout(timeout).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn returns_worker_value_before_deadline() {
        let result = run_with_timeout(Duration::from_millis(100), || 42);
        assert_eq!(result, Some(42));
    }

    #[test]
    fn returns_none_after_deadline() {
        let started = Instant::now();
        let result = run_with_timeout(Duration::from_millis(10), || {
            std::thread::sleep(Duration::from_millis(200));
            42
        });
        assert_eq!(result, None);
        assert!(started.elapsed() < Duration::from_millis(120));
    }

    #[test]
    fn returns_none_when_worker_panics() {
        let result = run_with_timeout(Duration::from_millis(100), || -> u8 {
            panic!("worker failed");
        });
        assert_eq!(result, None);
    }
}
