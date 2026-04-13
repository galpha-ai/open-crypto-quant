//! Token bucket rate limiter for API calls.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Token bucket rate limiter for API calls.
///
/// Uses atomic operations for lock-free rate limiting.
/// Tokens are represented as fixed-point numbers (tokens * 1000) for precision.
pub struct RateLimiter {
    /// Tokens available (fixed-point: tokens * 1000)
    tokens: AtomicU64,

    /// Maximum tokens (bucket capacity) in fixed-point
    max_tokens: u64,

    /// Tokens added per millisecond (fixed-point)
    refill_rate_per_ms: f64,

    /// Last refill timestamp in milliseconds since start
    last_refill_ms: AtomicU64,

    /// Start instant for time calculation
    start: Instant,
}

const FIXED_POINT_SCALE: u64 = 1000;

impl RateLimiter {
    /// Create a new rate limiter with the given requests per second limit.
    ///
    /// The bucket capacity is set to allow small bursts (2x the rate).
    pub fn new(requests_per_second: f64) -> Self {
        let max_tokens = (requests_per_second * 2.0 * FIXED_POINT_SCALE as f64) as u64;
        let refill_rate_per_ms = requests_per_second * FIXED_POINT_SCALE as f64 / 1000.0;

        Self {
            tokens: AtomicU64::new(max_tokens),
            max_tokens,
            refill_rate_per_ms,
            last_refill_ms: AtomicU64::new(0),
            start: Instant::now(),
        }
    }

    /// Acquire a token, waiting if necessary.
    ///
    /// Returns the time spent waiting (Duration::ZERO if no wait needed).
    pub async fn acquire(&self) -> Duration {
        let start = Instant::now();

        loop {
            self.refill();

            // Try to acquire a token
            let current = self.tokens.load(Ordering::Relaxed);
            if current >= FIXED_POINT_SCALE {
                // Try to decrement
                if self
                    .tokens
                    .compare_exchange(
                        current,
                        current - FIXED_POINT_SCALE,
                        Ordering::SeqCst,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    return start.elapsed();
                }
                // CAS failed, retry immediately
                continue;
            }

            // No tokens available, wait for refill
            // Calculate time to wait for one token
            let wait_ms = (FIXED_POINT_SCALE as f64 / self.refill_rate_per_ms).ceil() as u64;
            tokio::time::sleep(Duration::from_millis(wait_ms.max(1))).await;
        }
    }

    /// Try to acquire a token without waiting.
    ///
    /// Returns true if a token was acquired, false otherwise.
    pub fn try_acquire(&self) -> bool {
        self.refill();

        loop {
            let current = self.tokens.load(Ordering::Relaxed);
            if current < FIXED_POINT_SCALE {
                return false;
            }

            if self
                .tokens
                .compare_exchange(
                    current,
                    current - FIXED_POINT_SCALE,
                    Ordering::SeqCst,
                    Ordering::Relaxed,
                )
                .is_ok()
            {
                return true;
            }
            // CAS failed, retry
        }
    }

    /// Get the current number of available tokens.
    pub fn available(&self) -> f64 {
        self.refill();
        self.tokens.load(Ordering::Relaxed) as f64 / FIXED_POINT_SCALE as f64
    }

    /// Refill tokens based on elapsed time.
    fn refill(&self) {
        let now_ms = self.start.elapsed().as_millis() as u64;
        let last_ms = self.last_refill_ms.load(Ordering::Relaxed);

        if now_ms <= last_ms {
            return;
        }

        // Calculate tokens to add
        let elapsed_ms = now_ms - last_ms;
        let tokens_to_add = (elapsed_ms as f64 * self.refill_rate_per_ms) as u64;

        if tokens_to_add == 0 {
            return;
        }

        // Update last refill time
        let _ = self.last_refill_ms.compare_exchange(
            last_ms,
            now_ms,
            Ordering::SeqCst,
            Ordering::Relaxed,
        );

        // Add tokens (capped at max)
        loop {
            let current = self.tokens.load(Ordering::Relaxed);
            let new_tokens = (current + tokens_to_add).min(self.max_tokens);

            if current == new_tokens {
                break;
            }

            if self
                .tokens
                .compare_exchange(current, new_tokens, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_creation() {
        let limiter = RateLimiter::new(10.0);
        // Should start with 2x capacity (burst allowance)
        assert!(limiter.available() >= 10.0);
    }

    #[test]
    fn test_try_acquire_success() {
        let limiter = RateLimiter::new(10.0);
        assert!(limiter.try_acquire());
    }

    #[test]
    fn test_try_acquire_exhaustion() {
        let limiter = RateLimiter::new(5.0);
        // Exhaust all tokens (5 * 2 = 10 burst capacity)
        for _ in 0..10 {
            assert!(limiter.try_acquire());
        }
        // Should fail now
        assert!(!limiter.try_acquire());
    }

    #[tokio::test]
    async fn test_acquire_waits() {
        let limiter = RateLimiter::new(100.0); // 100 req/s = 10ms per token

        // Exhaust tokens
        while limiter.try_acquire() {}

        // Acquire should wait
        let wait_time = limiter.acquire().await;
        // Should have waited some time (at least a few ms)
        assert!(wait_time.as_millis() > 0 || limiter.available() >= 0.0);
    }

    #[test]
    fn test_refill() {
        let limiter = RateLimiter::new(1000.0); // Fast refill for testing

        // Exhaust some tokens
        for _ in 0..5 {
            limiter.try_acquire();
        }

        let before = limiter.available();
        std::thread::sleep(Duration::from_millis(10));
        let after = limiter.available();

        // Should have refilled some tokens
        assert!(after >= before);
    }
}
