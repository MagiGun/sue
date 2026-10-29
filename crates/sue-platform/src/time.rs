//! The clock. Everything above the platform reads the time through these
//! types: they are std's own on every system but the browser, where std's
//! clock panics and `web-time` reads the page's clock instead.

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::{Instant, SystemTime, UNIX_EPOCH};
#[cfg(target_arch = "wasm32")]
pub use web_time::{Instant, SystemTime, UNIX_EPOCH};

use std::sync::OnceLock;
use std::time::Duration;

/// The time since Sue first asked, on the monotonic clock: a plain
/// function for code below the platform that times its work (ingest).
#[must_use]
pub fn since_start() -> Duration {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_only_moves_forward() {
        let first = since_start();
        let second = since_start();
        assert!(second >= first);
        assert!(SystemTime::now().duration_since(UNIX_EPOCH).is_ok());
    }
}
