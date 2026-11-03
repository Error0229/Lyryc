//! Per-provider kill switch: after `TRIP_AFTER` consecutive failures a provider
//! is skipped for the rest of the session, so a dead or blocking endpoint
//! costs nothing. A working answer (even "not found") resets the count.

use super::Provider;
use std::sync::atomic::{AtomicU8, Ordering};

pub const TRIP_AFTER: u8 = 3;

pub struct Breaker([AtomicU8; Provider::ALL.len()]);

impl Breaker {
    pub const fn new() -> Self {
        Self([AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0)])
    }

    pub fn is_open(&self, p: Provider) -> bool {
        self.0[p as usize].load(Ordering::Relaxed) >= TRIP_AFTER
    }

    pub fn record(&self, p: Provider, ok: bool) {
        let n = &self.0[p as usize];
        if ok {
            // Once tripped, stay tripped: an endpoint that failed three times
            // in a row is not worth probing again this session.
            if n.load(Ordering::Relaxed) < TRIP_AFTER {
                n.store(0, Ordering::Relaxed);
            }
        } else if n.load(Ordering::Relaxed) < TRIP_AFTER {
            n.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trips_after_three_consecutive_failures() {
        let b = Breaker::new();
        b.record(Provider::Qq, false);
        b.record(Provider::Qq, false);
        assert!(!b.is_open(Provider::Qq));
        b.record(Provider::Qq, false);
        assert!(b.is_open(Provider::Qq));
        assert!(
            !b.is_open(Provider::Netease),
            "providers trip independently"
        );
        b.record(Provider::Qq, true);
        assert!(b.is_open(Provider::Qq), "stays open for the session");
    }

    #[test]
    fn success_resets_the_count() {
        let b = Breaker::new();
        for _ in 0..5 {
            b.record(Provider::Kugou, false);
            b.record(Provider::Kugou, false);
            b.record(Provider::Kugou, true);
        }
        assert!(!b.is_open(Provider::Kugou));
    }
}
