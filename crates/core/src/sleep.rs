use rand::Rng;
use std::thread;
use std::time::Duration;

/// Sleep for `secs` seconds with +/-30% random jitter.
pub fn sleep_jitter(secs: f64) {
    let jitter = secs * 0.3;
    let actual = secs + rand::thread_rng().gen_range(-jitter..jitter);
    thread::sleep(Duration::from_secs_f64(actual.max(0.01)));
}

/// Sleep for exact milliseconds (no jitter).
pub fn sleep_ms(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
