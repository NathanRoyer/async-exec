//! Future Timing Utilities

use std::future::Future;
use std::time::{Duration, Instant};

use async_io::Timer;
use futures_lite::future::or;

/// This trait adds a `timeout` method to all futures.
pub trait FutureTimeout<T>: Future<Output = T> + Sized {
    /// This method, implemented for all futures, returns another future which
    /// yields None if the inner future takes too long to yield its output.
    ///
    /// If the inner future yields in time, its output is in turn yielded in `Some(_)`.
    fn timeout(self, duration: Duration) -> impl Future<Output = Option<T>> {
        let this = async { Some(self.await) };

        let timeout = async move {
            sleep(duration).await;
            None
        };

        or(this, timeout)
    }
}

impl<T, F: Future<Output = T>> FutureTimeout<T> for F {}

/// Pauses the current task for some time.
pub async fn sleep(duration: Duration) {
    Timer::after(duration).await;
}

/// Pauses the current task until an expiration instant.
pub async fn sleep_until(instant: Instant) {
    Timer::at(instant).await;
}
