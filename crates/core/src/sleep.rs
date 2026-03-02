use rand::Rng;
use std::thread;
use std::time::Duration;

/// Sleep for exact milliseconds (no jitter).
pub fn ms(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}

/// Sleep for `base_ms` with +/-`percent` random jitter.
pub fn jittered_ms(base_ms: u64, percent: f64) {
    let base = base_ms as f64;
    let pct = if percent.is_finite() && percent >= 0.0 { percent } else { 0.3 };
    let jitter = base * pct;
    let actual = base + rand::thread_rng().gen_range(-jitter..jitter);
    let actual_ms = actual.max(1.0).round() as u64;
    thread::sleep(Duration::from_millis(actual_ms));
}
