#[cfg(web_platform)]
use wasm_bindgen::prelude::*;

use std::{collections::VecDeque, marker::PhantomData};

#[cfg(not(web_platform))]
use std::time as timecrate;

#[cfg(web_platform)]
use web_time as timecrate;

use timecrate::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[cfg(web_platform)]
#[wasm_bindgen(inline_js = r#"
export function _system_secs_f64() {
  return performance.now() / 1000.0;
}"#)]
extern "C" {
    fn _system_secs_f64() -> f64;
}

#[cfg(not(web_platform))]
fn _system_secs_f64() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

#[derive(Debug, Clone, Copy, PartialOrd, PartialEq, Serialize, Deserialize)]
pub struct LogicTime;

#[derive(Debug, Clone, Copy, PartialOrd, PartialEq, Serialize, Deserialize)]
pub struct RealTime;

#[derive(Debug, Clone, Copy, PartialOrd, PartialEq, Serialize, Deserialize)]
pub struct Timestamp<T>(f64, PhantomData<T>);

impl<T> Default for Timestamp<T> {
    fn default() -> Self {
        Self::zero()
    }
}

impl<T> Timestamp<T> {
    pub const fn zero() -> Self {
        Timestamp(0.0, PhantomData)
    }

    pub const fn secs(self) -> f64 {
        self.0
    }

    pub fn minus(self, amount: f64) -> Self {
        Timestamp(self.0 - amount, self.1)
    }

    pub fn plus(self, amount: f64) -> Self {
        Timestamp(self.0 + amount, self.1)
    }

    pub fn since(&self, other: Timestamp<T>) -> f64 {
        self.0 - other.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clock<T>(f64, PhantomData<T>);

impl<T> Clock<T> {
    pub const fn new_zero() -> Self {
        Clock(0.0, PhantomData)
    }

    pub fn new_now() -> Self {
        Clock(_system_secs_f64(), PhantomData)
    }

    pub const fn zero(&self) -> Timestamp<T> {
        Timestamp(0.0, PhantomData)
    }

    pub const fn secs(&self) -> f64 {
        self.0
    }

    pub fn set(&mut self, time: f64) -> f64 {
        let delta = time - self.0;
        self.0 = time;
        delta
    }

    pub fn update(&mut self) -> f64 {
        self.set(_system_secs_f64())
    }

    pub fn progress(&mut self, delta: f64) {
        self.0 += delta;
    }

    pub fn rewind(&mut self, delta: f64) {
        self.0 -= delta;
    }

    pub const fn now(&self) -> Timestamp<T> {
        Timestamp(self.0, self.1)
    }

    pub fn since(&self, time: Timestamp<T>) -> f64 {
        self.0 - time.0
    }

    pub fn until(&self, time: Timestamp<T>) -> f64 {
        -self.since(time)
    }
}

pub type LogicClock = Clock<LogicTime>;
pub type LogicTimestamp = Timestamp<LogicTime>;
pub type RealClock = Clock<RealTime>;
pub type RealTimestamp = Timestamp<RealTime>;

pub struct FramePacer {
    frame_time: Instant,
}

impl FramePacer {
    pub fn new() -> FramePacer {
        FramePacer {
            frame_time: Instant::now(),
        }
    }

    pub fn deadline_for_fps(&mut self, fps: f64) -> Instant {
        let now = Instant::now();

        let target_frame_duration = 1. / fps;
        let frame_deadline = self.frame_time + Duration::from_secs_f64(target_frame_duration);

        if now < frame_deadline {
            self.frame_time = frame_deadline;
        } else {
            self.frame_time = now;
        }

        frame_deadline
    }
}

pub struct FpsCounter {
    frames: VecDeque<Duration>,
    last_t: Instant,
    window_size: usize,
}

impl FpsCounter {
    pub fn new(window_size: usize, start_t: Instant) -> Self {
        FpsCounter {
            frames: VecDeque::new(),
            last_t: start_t,
            window_size,
        }
    }

    pub fn new_now(window_size: usize) -> Self {
        let now = Instant::now();
        Self::new(window_size, now)
    }

    pub fn update(&mut self, now: Instant) {
        let dt = now.duration_since(self.last_t);
        self.last_t = now;
        self.frames.push_back(dt);
        if self.frames.len() > self.window_size {
            self.frames.pop_front();
        }
    }

    pub fn update_now(&mut self) {
        let now = Instant::now();
        self.update(now);
    }

    pub fn mean_frame_time(&self) -> Duration {
        self.frames.iter().sum::<Duration>() / self.frames.len() as u32
    }

    pub fn mean_fps(&self) -> f64 {
        let mean_frame_secs = self.mean_frame_time().as_secs_f64();
        1. / mean_frame_secs
    }

    pub fn max_frame_time(&self) -> Option<Duration> {
        self.frames.iter().max().copied()
    }

    pub fn min_fps(&self) -> Option<f64> {
        self.max_frame_time().map(|x| 1. / x.as_secs_f64())
    }
}
