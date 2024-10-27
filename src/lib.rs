//! # SimpleHighPrecisionClock
//!
//! `SimpleHighPrecisionClock` provides a high-precision clock that leverages the CPU's
//! Time Stamp Counter (TSC) to measure time elapsed since instantiation in nanoseconds.
//!
//! The idea is from `tscns` a very impressive and lightweight clock in C.
//!
//! This clock is calibrated during initialization to convert TSC ticks to nanoseconds
//! independently of CPU frequency, ensuring high precision and consistent measurements.
//!
//! The `calibrate` function should be called from time to time (1 second, for example)
//! to adjust the base tsc and time to keep the precision.
//!
//! ## Example
//!
//! ```rust
//! use high_precision_clock::SimpleHighPrecisionClock;
//!
//! let mut clock = SimpleHighPrecisionClock::new(100 * 1000 * 1000);
//! let time_ns = clock.now();
//! println!("Elapsed time in nanoseconds: {}", time_ns);
//! loop {
//!     clock.calibrate();
//!     // your task
//!}
//! ```
//!
//! This library is particularly useful for applications that require precise time
//! tracking in environments where traditional time sources may lack stability or
//! granularity.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

fn get_time() -> u64 {
    // Reads the Time Stamp Counter (TSC)
    unsafe { core::arch::x86_64::_rdtsc() }
}

fn rdsysns() -> u64 {
    // Reads the current system time in nanoseconds
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos() as u64
}

pub struct SimpleHighPrecisionClock {
    base_tsc: AtomicU64,
    base_ns: AtomicU64,
    ns_per_tsc: f64,
    calibration_interval_ns: u64,
    base_ns_err: i64,
    next_calibrate_tsc: u64,
}

impl SimpleHighPrecisionClock {
    /// Initializes the clock and performs an initial calibration.
    pub fn new(calibration_interval_ns: u64) -> Self {
        let (base_tsc, base_ns, ns_per_tsc) = Self::calibrate_once();
        let next_calibrate_tsc = base_tsc + (calibration_interval_ns as f64 / ns_per_tsc) as u64;
        Self {
            base_tsc: AtomicU64::new(base_tsc),
            base_ns: AtomicU64::new(base_ns),
            ns_per_tsc,
            calibration_interval_ns,
            base_ns_err: 0,
            next_calibrate_tsc,
        }
    }

    /// Calibrates the TSC using a technique that adjusts `ns_per_tsc` based on observed drift.
    pub fn calibrate(&mut self) {
        let current_tsc = get_time();
        if current_tsc < self.next_calibrate_tsc {
            return;
        }

        let (tsc, ns) = Self::sync_time();
        let calculated_ns = self.tsc_to_ns(tsc);
        let ns_err = calculated_ns as i64 - ns as i64;

        // Estimate error drift for next calibration.
        let expected_err_next = ns_err
            + (ns_err - self.base_ns_err) * self.calibration_interval_ns as i64
                / (ns as i64 - self.base_ns.load(Ordering::SeqCst) as i64 + self.base_ns_err);

        // Update `ns_per_tsc` based on error estimate.
        self.ns_per_tsc *= 1.0 - (expected_err_next as f64 / self.calibration_interval_ns as f64);
        self.save_params(tsc, calculated_ns, ns_err);
    }

    /// Converts TSC to nanoseconds based on `ns_per_tsc`.
    #[inline]
    fn tsc_to_ns(&self, tsc: u64) -> u64 {
        let elapsed_cycles = tsc - self.base_tsc.load(Ordering::SeqCst);
        self.base_ns.load(Ordering::SeqCst) + (elapsed_cycles as f64 * self.ns_per_tsc) as u64
    }

    /// Performs an initial calibration by observing elapsed TSC over a short interval.
    fn calibrate_once() -> (u64, u64, f64) {
        let base_tsc = get_time();
        let base_ns = rdsysns();
        std::thread::sleep(std::time::Duration::from_millis(20));

        let new_tsc = get_time();
        let new_ns = rdsysns();

        let ns_per_tsc = (new_ns - base_ns) as f64 / (new_tsc - base_tsc) as f64;
        (base_tsc, base_ns, ns_per_tsc)
    }

    /// Synchronizes TSC with system time, attempting multiple times to minimize TSC drift.
    fn sync_time() -> (u64, u64) {
        const SYNC_ATTEMPTS: usize = 10; // Number of attempts to synchronize

        let mut min_diff = u64::MAX;
        let mut best_tsc = 0;
        let mut best_ns = 0;

        for _ in 0..SYNC_ATTEMPTS {
            let tsc_before = get_time();
            let ns = rdsysns();
            let tsc_after = get_time();

            let diff = tsc_after - tsc_before;
            if diff < min_diff {
                min_diff = diff;
                best_tsc = (tsc_before + tsc_after) / 2;
                best_ns = ns;
            }
        }
        (best_tsc, best_ns)
    }

    /// Updates parameters after each calibration.
    fn save_params(&mut self, tsc: u64, calculated_ns: u64, ns_err: i64) {
        self.base_ns_err = ns_err;
        self.next_calibrate_tsc =
            tsc + (self.calibration_interval_ns as f64 / self.ns_per_tsc) as u64;
        self.base_tsc.store(tsc, Ordering::SeqCst);
        self.base_ns.store(calculated_ns, Ordering::SeqCst);
    }

    /// Returns the current time in nanoseconds since the UNIX epoch.
    pub fn now(&self) -> u64 {
        let current_tsc = get_time();
        self.tsc_to_ns(current_tsc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn test_initialization() {
        let clock = SimpleHighPrecisionClock::new(10000);
        let time_ns = clock.now();
        assert!(time_ns > 0, "The initial time should be positive.");
    }

    #[test]
    fn test_increasing_time() {
        let clock = SimpleHighPrecisionClock::new(10000);
        let time_ns1 = clock.now();

        // Increase sleep to ensure time passes enough for `now` to update
        sleep(Duration::from_millis(100));

        let time_ns2 = clock.now();
        assert!(
            time_ns2 > time_ns1,
            "Time should increase with each call to now: {} vs. {}",
            time_ns2,
            time_ns1
        );
    }

    #[test]
    fn test_drift_with_consecutive_calls() {
        let clock = SimpleHighPrecisionClock::new(10000);
        let time_ns1 = clock.now();

        // Increase sleep duration significantly
        sleep(Duration::from_millis(100));

        let time_ns2 = clock.now();
        let drift = time_ns2 - time_ns1;

        // Larger tolerance for the new sleep interval
        assert!(
            drift > 90_000_000 && drift < 110_000_000,
            "Drift should be close to 100 milliseconds but was {} nanoseconds",
            drift
        );
    }

    #[test]
    fn test_drift_with_calibration() {
        // Test if calling `calibrate` reduces drift after a delay.
        let mut clock = SimpleHighPrecisionClock::new(3_000_000_000);

        // Simulate time passage and accumulate drift
        let initial_time_ns = clock.now();
        sleep(Duration::from_secs(2)); // Sleep for 2 seconds to allow drift
        let time_ns_after_drift = clock.now();
        let drift_before_calibration = (time_ns_after_drift - initial_time_ns) as i64 - 2_000_000_000;

        // Call calibrate to adjust for drift
        clock.calibrate();

        // After calibration, drift should be minimized
        let time_ns_after_calibration = clock.now();
        sleep(Duration::from_secs(2)); // Sleep for 2 seconds to allow post-calibration drift
        let time_ns_final = clock.now();
        let drift_after_calibration = (time_ns_final - time_ns_after_calibration) as i64 - 2_000_000_000;

        assert!(
            drift_after_calibration.abs() < drift_before_calibration.abs(),
            "Drift should be reduced after calibration."
        );
    }

    #[test]
    fn test_multiple_calibrations() {
        // Test that multiple calls to `calibrate` maintain reasonable accuracy.
        let mut clock = SimpleHighPrecisionClock::new(1_000_000_000); // Set calibration interval to 1 second
        let mut previous_ns_per_tsc = clock.ns_per_tsc;

        for _ in 0..5 {
            // Simulate time passage
            sleep(Duration::from_secs(1));
            clock.calibrate();

            // Check if `ns_per_tsc` was updated, indicating calibration occurred
            let current_ns_per_tsc = clock.ns_per_tsc;
            assert_ne!(
                previous_ns_per_tsc, current_ns_per_tsc,
                "ns_per_tsc should adjust on each calibration call"
            );

            // Update previous value for next comparison
            previous_ns_per_tsc = current_ns_per_tsc;
        }
    }
}
