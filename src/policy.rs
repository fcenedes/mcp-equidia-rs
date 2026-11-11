use std::{sync::Mutex, time::Instant};

pub(crate) struct RateLimit {
    per_minute: u32,
    state: Mutex<(Instant, f64)>,
}

impl RateLimit {
    pub fn new(per_minute: u32) -> Self {
        Self {
            per_minute,
            state: Mutex::new((Instant::now(), f64::from(per_minute))),
        }
    }

    pub fn check(&self) -> Option<u64> {
        if self.per_minute == 0 {
            return None;
        }
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now().max(state.0);
        let rate = f64::from(self.per_minute) / 60.;
        state.1 = (state.1 + now.duration_since(state.0).as_secs_f64() * rate)
            .min(f64::from(self.per_minute));
        state.0 = now;
        if state.1 >= 1. {
            state.1 -= 1.;
            None
        } else {
            Some(((1. - state.1) / rate).ceil() as u64)
        }
    }
}
