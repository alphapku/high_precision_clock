# SimpleHighPrecisionClock

`SimpleHighPrecisionClock` is a high-precision time source that uses the CPU's Time Stamp Counter (TSC) to measure time elapsed since instantiation in nanoseconds.

The idea is from `tscns` a very impressive and lightweight clock in C.

This clock is calibrated during initialization to convert TSC ticks to nanoseconds independently of CPU frequency, ensuring high precision and consistent measurements.

The `calibrate` function should be called from time to time (1 second, for example) to adjust the base tsc and time to keep the precision.

## Example
```rust
let mut clock = SimpleHighPrecisionClock::new(500*1000*1000);
let time_ns = clock.now();
println!("Elapsed time in nanoseconds: {}", time_ns);
loop {
    clock.calibrate();
    // your task
}
