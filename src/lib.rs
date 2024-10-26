//! # HighPrecisionClock
//!
//! A high-precision clock module optimized for cloud server environments, designed for applications requiring
//! accurate time-tracking and drift correction. It optionally leverages the Time Stamp Counter (TSC) for
//! high-frequency clock updates on x86_64 Linux systems. This module is particularly suited for cloud-based
//! trading systems, real-time analytics, and other latency-sensitive applications.
//!
//! ## Important Note
//! This crate assumes that `chrony` is running on the server to regularly synchronize
//! the system clock. Proper system time synchronization is crucial, especially in cloud environments, to ensure
//! that the high-precision clock remains accurate and drift is minimized over time.
//!
//! ## Features
//! - **High Precision Time Measurement**: Provides high-resolution time measurement using `chrono::DateTime<Utc>`.
//! - **TSC Support**: Optionally utilizes the TSC for fast, cycle-based time calculations on compatible systems,
//!   falling back to `std::time::Instant` if TSC is not available or if the `tsc` feature is disabled. This is
//!   especially useful on cloud servers with invariant TSC support, allowing consistent timing across virtual cores.
//! - **Drift Correction**: Periodically recalibrates the time baselines to minimize drift between system time and
//!   the high-precision clock, with configurable calibration intervals and warning thresholds. Optimized for
//!   long-running cloud environments where clock drift is a concern.
//! - **Configurable Synchronous/Asynchronous Modes**: Offers separate implementations for async and non-async
//!   environments, utilizing either `tokio::RwLock` or `std::sync::RwLock` depending on the `async` feature.
//!
//! ## Usage
//!
//! ```rust
//! // Synchronous Mode (Non-async) for Cloud Environment
//! use high_precision_clock::HighPrecisionClock;
//! use std::sync::Arc;
//! use std::time::Duration;
//!
//! let clock = HighPrecisionClock::new(100_000, Duration::from_secs(1));
//! let current_time = clock.read().unwrap().now();
//! println!("Current time: {:?}", current_time);
//! ```
//!
//! ## Feature Flags
//! - `async`: Enables asynchronous mode, using `tokio::RwLock` and `tokio::time::sleep` for drift correction.
//! - `tsc`: Enables use of the Time Stamp Counter (TSC) on x86_64 Linux for high-frequency, cycle-based timing.
//!
//! ## Implementation Details
//!
//! ### `HighPrecisionClock` Struct
//! The `HighPrecisionClock` struct maintains a baseline time using either `Instant` (non-TSC) or TSC for fast elapsed
//! time calculation. It is periodically recalibrated to correct drift relative to the system time, a vital feature for
//! cloud-based environments where VM time drift may occur over extended runtime.
//!
//! ### Drift Correction
//! Drift correction is handled by the `periodic_drift_correction` function, which is either an async task (if the
//! `async` feature is enabled) or a regular thread. This function periodically checks the clock against the system
//! time and resets the baselines if drift exceeds a user-defined warning threshold. The feature is designed with
//! cloud server uptime in mind, addressing the long-running process needs typical in cloud-native applications.
//!
//! ### TSC Invariance Check
//! For systems where TSC is used, the `check_invariant_tsc` function validates the presence of the `constant_tsc` and
//! `nonstop_tsc` CPU flags, ensuring that the TSC operates reliably as a stable, high-frequency clock. This is crucial
//! for virtualized cloud servers where invariant TSC can provide consistent timing across virtual cores.
//!
//! ## Example Configuration
//!
//! ```rust
//! // Asynchronous Mode with TSC (if enabled and compatible) for Cloud Server
//! #[tokio::main]
//! async fn main() {
//!     let clock = HighPrecisionClock::new(100_000, Duration::from_secs(1));
//!     let current_time = clock.read().await.now();
//!     println!("Current time: {:?}", current_time);
//! }
//! ```
//!
//! ## Note
//! This crate is intended for high-performance, cloud-based environments, such as trading systems, real-time analytics,
//! or any other applications requiring strict timing consistency. Users are advised to check for TSC compatibility
//! and invariance on their cloud server instances to fully leverage the performance benefits of this crate.
//!
//! ## Dependency
//! This crate expects `chrony` or a similar NTP client to be running to keep the system time in sync.

use std::sync::Arc;
use std::time::Duration; // Use for thread sleep and time-based operations

#[cfg(feature = "async")]
use tokio::time::sleep;
#[cfg(not(feature = "async"))]
use std::thread::sleep;

#[cfg(feature = "async")]
use tokio::sync::RwLock; // Async lock in async context
#[cfg(not(feature = "async"))]
use std::sync::RwLock; // Standard lock in sync context

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use log::*;

#[cfg(all(feature = "tsc", target_os = "linux", target_arch = "x86_64"))]
use core::arch::x86_64::_rdtsc;

#[cfg(all(feature = "tsc", target_os = "linux", target_arch = "x86_64"))]
fn get_time() -> u64 {
    unsafe { _rdtsc() }
}

#[cfg(not(all(feature = "tsc", target_os = "linux", target_arch = "x86_64")))]
use std::time::Instant;

pub struct HighPrecisionClock {
    #[cfg(not(all(feature = "tsc", target_os = "linux", target_arch = "x86_64")))]
    base_instant: Instant, // Monotonic time baseline

    #[cfg(all(feature = "tsc", target_os = "linux", target_arch = "x86_64"))]
    base_tsc: u64, // TSC baseline

    base_datetime: DateTime<Utc>, // Baseline datetime in UTC

    #[cfg(all(feature = "tsc", target_os = "linux", target_arch = "x86_64"))]
    cpu_frequency_hz: f64, // CPU frequency in Hz

    warning_threshold_ns: i64,
}

impl HighPrecisionClock {
    pub fn new(warning_threshold_ns: i64, calibration_int: Duration) -> Arc<RwLock<Self>> {
        let clock = Self::initialize_clock(warning_threshold_ns);
        let clock = Arc::new(RwLock::new(clock));

        let clock_clone = Arc::clone(&clock);

        if let Some(invariant_tsc) = Self::check_invariant_tsc() {
            if !invariant_tsc {
                error!("TSC is NOT invariant while TSC mode is enabled");
            }
        }

        #[cfg(feature = "async")]
        tokio::spawn(async move {
            info!("Starting time drift correction (async)");
            Self::periodic_drift_correction(calibration_int, clock_clone).await;
        });
        #[cfg(not(feature = "async"))]
        std::thread::spawn(move || {
            info!("Starting time drift correction (sync)");
            Self::periodic_drift_correction(calibration_int, clock_clone);
        });

        clock
    }

    fn check_invariant_tsc() -> Option<bool> {
        #[cfg(all(feature = "tsc", target_os = "linux", target_arch = "x86_64"))]
        {
            if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
                return Some(cpuinfo.contains("constant_tsc") && cpuinfo.contains("nonstop_tsc"));
            }
            return Some(false);
        }
        None
    }

    fn initialize_clock(warning_threshold_ns: i64) -> Self {
        #[cfg(all(feature = "tsc", target_os = "linux", target_arch = "x86_64"))]
        {
            let base_datetime = Utc::now();
            let base_tsc = get_time();
            let cpu_frequency_hz = Self::get_cpu_frequency();
            Self {
                base_tsc,
                base_datetime,
                cpu_frequency_hz,
                warning_threshold_ns,
            }
        }

        #[cfg(not(all(feature = "tsc", target_os = "linux", target_arch = "x86_64")))]
        {
            let base_datetime = Utc::now();
            let base_instant = Instant::now();
            Self {
                base_instant,
                base_datetime,
                warning_threshold_ns,
            }
        }
    }

    fn get_cpu_frequency() -> f64 {
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo")
            .expect("Failed to read /proc/cpuinfo");

        for line in cpuinfo.lines() {
            if line.starts_with("cpu MHz") {
                if let Some(freq_str) = line.split(':').nth(1) {
                    let freq_mhz: f64 = freq_str.trim().parse().expect("Failed to parse CPU frequency");
                    info!("The CPU frequency is: {} MHz", freq_mhz);
                    return freq_mhz * 1_000_000.0;
                }
            }
        }
        panic!("Could not determine CPU frequency from /proc/cpuinfo");
    }

    pub fn now(&self) -> DateTime<Utc> {
        #[cfg(all(feature = "tsc", target_os = "linux", target_arch = "x86_64"))]
        {
            let current_tsc = get_time();
            let elapsed_cycles = current_tsc - self.base_tsc;
            let elapsed_ns = (elapsed_cycles as f64 / self.cpu_frequency_hz) * 1_000_000_000.0;
            self.base_datetime + ChronoDuration::nanoseconds(elapsed_ns as i64)
        }

        #[cfg(not(all(feature = "tsc", target_os = "linux", target_arch = "x86_64")))]
        {
            let elapsed = self.base_instant.elapsed();
            let elapsed_ns = elapsed.as_nanos() as i64;
            self.base_datetime + ChronoDuration::nanoseconds(elapsed_ns)
        }
    }

    fn reset_baselines(&mut self, desc: &str) {
        let sys_time = Utc::now();
        let drift = sys_time - self.now();

        self.base_datetime = sys_time;

        #[cfg(not(all(feature = "tsc", target_os = "linux", target_arch = "x86_64")))]
        {
            self.base_instant = Instant::now();
        }

        #[cfg(all(feature = "tsc", target_os = "linux", target_arch = "x86_64"))]
        {
            self.base_tsc = get_time();
        }

        let drift_ns = drift.num_nanoseconds().unwrap_or_default().abs();
        if  drift_ns >= self.warning_threshold_ns {
            warn!("Significant time drift detected ({}): {:?}(ns)", desc, drift_ns);
        } else {
            trace!("Time drift detected ({}): {:?}(ns)", desc, drift_ns);
        }
    }

    #[cfg(feature = "async")]
    async fn periodic_drift_correction(calibration_int: Duration, clock: Arc<RwLock<Self>>) {
        loop {
            sleep(calibration_int).await;

            // Obtain a write lock directly without using match
            let mut clock = clock.write().await;

            // // Update or correct drift logic here
            // let estimated_time = clock.now();
            // let current_datetime = Utc::now();
            // let drift = current_datetime - estimated_time;
            //
            // // Adjust clock base values based on drift here
            // clock.base_datetime = current_datetime;

            clock.reset_baselines("async");
        }
    }


    #[cfg(not(feature = "async"))]
    fn periodic_drift_correction(calibration_int: Duration, clock: Arc<RwLock<Self>>) {
        loop {
            sleep(calibration_int);

            let mut clock = match clock.write() {
                Ok(clock) => clock,
                Err(e) => {
                    error!("Failed to acquire clock lock during calibration: {:?}", e);
                    continue;
                }
            };

            // let estimated_time = clock.now();
            // let current_datetime = Utc::now();
            // let drift = current_datetime - estimated_time;
            //
            // clock.base_datetime = current_datetime;

            clock.reset_baselines("sync");
        }
    }
}
