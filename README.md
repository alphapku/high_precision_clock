# SimpleHighPrecisionClock

`SimpleHighPrecisionClock` provides a simple but very fast high-precision clock based on the CPU's Time Stamp Counter (TSC). It allows tracking time in any timezone, which is specified upon creation.

This clock uses the TSC to calculate precise time intervals by comparing current TSC readings to an initial baseline TSC reading. The CPU frequency is used to convert the TSC difference into nanoseconds, which is then added to a baseline date-time set at instantiation.

This clock assumes that the TSC is constant and non-stop on the system. A warning is issued if the TSC appears to be non-invariant, as this may reduce accuracy.

## Example
```rust
use chrono::Utc;
use high_precision_clock::SimpleHighPrecisionClock;

let clock = SimpleHighPrecisionClock::new(&Utc);
let current_time = clock.now();
println!("Current high-precision time: {}", current_time);
