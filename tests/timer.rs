use std::{thread, time::Duration};

mod timer {
    include!("../src/utils/timer.rs");
}
use timer::Timer;

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
