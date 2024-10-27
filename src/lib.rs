/// `SimpleHighPrecisionClock` provides a high-precision clock based on the CPU's
/// Time Stamp Counter (TSC). It allows you to track time in any timezone, which
/// is specified upon creation.
///
/// This clock uses the TSC to calculate precise time intervals by comparing
/// current TSC readings to an initial baseline TSC reading. The CPU frequency
/// is used to convert the TSC difference into nanoseconds, which is added to
/// a baseline date-time set at instantiation.
///
/// This clock assumes that the TSC is constant and non-stop on the system.
/// It warns if the TSC appears to be non-invariant, as this may reduce accuracy.
///
/// # Type Parameters
/// - `Tz`: A timezone that implements `TimeZone`, allowing the clock to
///         provide time outputs in the specified timezone.
///
/// # Example
/// ```rust
/// use chrono::Utc;
/// use high_precision_clock::SimpleHighPrecisionClock;
/// let clock = SimpleHighPrecisionClock::new(&Utc);
/// let current_time = clock.now();
/// println!("Current high-precision time: {}", current_time);
/// ```


use std::marker::PhantomData;

use chrono::{DateTime, Duration, TimeZone, Utc};
use log::{info, warn};

fn get_time() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}

pub struct SimpleHighPrecisionClock<Tz: TimeZone> {
    base_tsc: u64,
    base_datetime: DateTime<Tz>,
    cpu_frequency_hz: f64,
    _timezone: PhantomData<Tz>,
}

impl<Tz: TimeZone> SimpleHighPrecisionClock<Tz> {
    pub fn new(tz: &Tz) -> Self {
        if !std::fs::read_to_string("/proc/cpuinfo").map_or(false, |v| {
            v.contains("constant_tsc") && v.contains("nonstop_tsc")
        }) {
            warn!("TSC is NOT invariant");
        }

        let base_datetime = tz.from_utc_datetime(&Utc::now().naive_utc());
        let base_tsc = get_time();
        let cpu_frequency_hz = Self::get_cpu_frequency();
        Self {
            base_tsc,
            base_datetime,
            cpu_frequency_hz,
            _timezone: PhantomData,
        }
    }

    fn get_cpu_frequency() -> f64 {
        let cpuinfo =
            std::fs::read_to_string("/proc/cpuinfo").expect("Failed to read /proc/cpuinfo");

        for line in cpuinfo.lines() {
            if line.starts_with("cpu MHz") {
                if let Some(freq_str) = line.split(':').nth(1) {
                    let freq_mhz: f64 = freq_str
                        .trim()
                        .parse()
                        .expect("Failed to parse CPU frequency");
                    info!("The CPU frequency is: {} MHz", freq_mhz);
                    return freq_mhz * 1_000_000.0;
                }
            }
        }
        panic!("Could not determine CPU frequency from /proc/cpuinfo");
    }

    pub fn now(&self) -> DateTime<Tz> {
        let current_tsc = get_time();
        let elapsed_cycles = current_tsc - self.base_tsc;
        let elapsed_ns = (elapsed_cycles as f64 / self.cpu_frequency_hz) * 1_000_000_000.0;
        self.base_datetime.clone() + Duration::nanoseconds(elapsed_ns as i64)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use super::SimpleHighPrecisionClock;
    use chrono::{Utc, Local};

    #[test]
    fn test_clock_initialization() {
        let clock = SimpleHighPrecisionClock::new(&Utc);
        let start_time = clock.now();
        assert!(start_time.timestamp() > 0, "Clock should initialize with a positive timestamp.");
    }

    #[test]
    fn test_monotonic_behavior() {
        let clock = SimpleHighPrecisionClock::new(&Utc);
        let time1 = clock.now();
        let time2 = clock.now();
        assert!(time2 > time1, "Clock should be monotonic; second reading should be greater.");
    }

    #[test]
    fn test_consistent_with_system_time() {
        let clock = SimpleHighPrecisionClock::new(&Utc);
        let custom_time = clock.now();
        let system_time = Utc::now();

        // Allow a reasonable drift tolerance (e.g., 1 second)
        let drift = (system_time - custom_time).num_milliseconds().abs();
        assert!(drift < 1000, "Custom clock and system time should have minimal drift.");
    }

    #[test]
    fn test_drift() {
        let clock = SimpleHighPrecisionClock::new(&Local);
        let simple_now = clock.now();
        let sys_now = Local::now();
        std::thread::sleep(Duration::from_secs(3));
        println!("drift1:{} <-> drift2: {}", clock.now().signed_duration_since(simple_now), clock.now().signed_duration_since(sys_now));
    }
}
