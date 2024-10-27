# SimpleHighPrecisionClock

`SimpleHighPrecisionClock` is a high-precision time source that uses the CPU's Time Stamp Counter (TSC) to measure time elapsed since instantiation in nanoseconds.

This clock calibrates the TSC upon initialization, converting TSC ticks to nanoseconds without relying on the CPU frequency, ensuring greater precision and stability.

## Example
```rust
let clock = SimpleHighPrecisionClock::new();
let time_ns = clock.now();
println!("Elapsed time in nanoseconds: {}", time_ns);

