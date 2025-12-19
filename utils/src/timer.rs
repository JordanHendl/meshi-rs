use std::time::{Duration, Instant};
pub struct Timer {
    start_time: Option<Instant>,
    elapsed: Duration,
    is_paused: bool,
}

impl Timer {
    // Create a new timer instance
    pub fn new() -> Timer {
        Timer {
            start_time: None,
            elapsed: Duration::new(0, 0),
            is_paused: false,
        }
    }

    // Start the timer
    pub fn start(&mut self) {
        if self.is_paused {
            // Resume from where it was paused
            self.start_time = Some(Instant::now() - self.elapsed);
            self.is_paused = false;
        } else {
            // Start or restart the timer
            self.start_time = Some(Instant::now());
            self.elapsed = Duration::new(0, 0);
        }
    }

    // Stop the timer
    pub fn stop(&mut self) {
        if self.start_time.is_some() {
            self.elapsed = self.elapsed_duration();
            self.start_time = None;
            self.is_paused = false;
        }
    }

    // Pause the timer
    pub fn pause(&mut self) {
        if self.start_time.is_some() {
            self.elapsed = self.elapsed_duration();
            self.is_paused = true;
            self.start_time = None;
        }
    }

    // Reset the timer
    pub fn reset(&mut self) {
        self.start_time = None;
        self.elapsed = Duration::new(0, 0);
        self.is_paused = false;
    }

    // Get the current elapsed duration
    pub fn elapsed_duration(&self) -> Duration {
        if let Some(start_time) = self.start_time {
            if self.is_paused {
                self.elapsed
            } else {
                start_time.elapsed()
            }
        } else {
            self.elapsed
        }
    }

    // Get the current elapsed time in milliseconds
    pub fn elapsed_ms(&self) -> u128 {
        self.elapsed_duration().as_millis()
    }

    // Get the current elapsed time in microseconds
    pub fn elapsed_micro(&self) -> u128 {
        self.elapsed_duration().as_micros()
    }

    // Get the current elapsed time in seconds as f32
    pub fn elapsed_seconds_f32(&self) -> f32 {
        self.elapsed_duration().as_secs_f32()
    }

    // Get the current elapsed time in seconds as f64
    pub fn elapsed_seconds_f64(&self) -> f64 {
        self.elapsed_duration().as_secs_f64()
    }
}

#[cfg(test)]
mod tests {
    use super::Timer;
    use std::{thread, time::Duration};

    #[test]
    fn start_records_elapsed_time() {
        let mut timer = Timer::new();
        timer.start();
        thread::sleep(Duration::from_millis(10));
        assert!(timer.elapsed_ms() >= 10);
    }

    #[test]
    fn pause_freezes_elapsed_time() {
        let mut timer = Timer::new();
        timer.start();
        thread::sleep(Duration::from_millis(10));
        timer.pause();
        let paused = timer.elapsed_ms();
        thread::sleep(Duration::from_millis(10));
        assert_eq!(paused, timer.elapsed_ms());
    }

    #[test]
    fn resume_continues_after_pause() {
        let mut timer = Timer::new();
        timer.start();
        thread::sleep(Duration::from_millis(10));
        timer.pause();
        let paused = timer.elapsed_ms();
        thread::sleep(Duration::from_millis(10));
        assert_eq!(paused, timer.elapsed_ms());

        timer.start(); // resume
        thread::sleep(Duration::from_millis(10));
        assert!(timer.elapsed_ms() > paused);
    }

    #[test]
    fn stop_stops_elapsed_time() {
        let mut timer = Timer::new();
        timer.start();
        thread::sleep(Duration::from_millis(10));
        timer.stop();
        let stopped = timer.elapsed_ms();
        thread::sleep(Duration::from_millis(10));
        assert_eq!(stopped, timer.elapsed_ms());
    }

    #[test]
    fn reset_clears_elapsed_time() {
        let mut timer = Timer::new();
        timer.start();
        thread::sleep(Duration::from_millis(10));
        timer.reset();
        assert_eq!(timer.elapsed_ms(), 0);

        timer.start();
        thread::sleep(Duration::from_millis(10));
        assert!(timer.elapsed_ms() >= 10);
    }

    #[test]
    fn elapsed_convenience_methods() {
        let mut timer = Timer::new();
        timer.start();
        thread::sleep(Duration::from_micros(10));
        assert!(timer.elapsed_micro() >= 10);
        assert!(timer.elapsed_seconds_f32() > 0.0);
        assert!(timer.elapsed_seconds_f64() > 0.0);
    }
}
