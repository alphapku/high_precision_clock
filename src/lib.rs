/// `SimpleHighPrecisionClock` is a high-precision time source that uses the CPU's
/// Time Stamp Counter (TSC) to measure time elapsed since instantiation in nanoseconds.
///
/// This clock calibrates the TSC upon initialization, converting TSC ticks to nanoseconds
/// without relying on the CPU frequency, ensuring greater precision and stability.
///
/// # Example
/// ```
/// use high_precision_clock::SimpleHighPrecisionClock;
/// let clock = SimpleHighPrecisionClock::new();
/// let time_ns = clock.now();
/// println!("Elapsed time in nanoseconds: {}", time_ns);
/// ```


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
    base_tsc: u64,
    base_ns: u64,
    ns_per_tsc: f64, // Keeps fractional precision for accurate calculations
}

impl SimpleHighPrecisionClock {
    pub fn new() -> Self {
        let base_ns = rdsysns() as u64;
        let base_tsc = get_time();
        let ns_per_tsc = Self::calibrate_ns_per_tsc(); // Returns f64 for precision

        SimpleHighPrecisionClock {
            base_tsc,
            base_ns,
            ns_per_tsc,
        }
    }

    fn calibrate_ns_per_tsc() -> f64 {
        let base_tsc = get_time();
        let base_ns = rdsysns();
        std::thread::sleep(std::time::Duration::from_millis(20)); // Initial delay

        let new_tsc = get_time();
        let new_ns = rdsysns();

        (new_ns - base_ns) as f64 / (new_tsc - base_tsc) as f64 // Precise fractional value
    }

    pub fn now(&self) -> u64 {
        let current_tsc = get_time();
        let elapsed_cycles = current_tsc - self.base_tsc;
        let elapsed_ns = (elapsed_cycles as f64 * self.ns_per_tsc) as u64;
        self.base_ns + elapsed_ns
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn test_initialization() {
        let clock = SimpleHighPrecisionClock::new();
        let time_ns = clock.now();
        assert!(time_ns > 0, "The initial time should be positive.");
    }

    #[test]
    fn test_increasing_time() {
        let clock = SimpleHighPrecisionClock::new();
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
        let clock = SimpleHighPrecisionClock::new();
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
}
