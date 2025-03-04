#[cfg(web_platform)]
use wasm_bindgen::prelude::*;

use std::marker::PhantomData;

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
    frame_time: timecrate::Instant,
}

impl FramePacer {
    pub fn new() -> FramePacer {
        FramePacer {
            frame_time: timecrate::Instant::now(),
        }
    }

    pub fn deadline_for_fps(&mut self, fps: f64) -> timecrate::Instant {
        let now = timecrate::Instant::now();

        let target_frame_duration = 1. / fps;
        let frame_deadline = self.frame_time + timecrate::Duration::from_secs_f64(target_frame_duration);

        if now < frame_deadline {
            self.frame_time = frame_deadline;
        } else {
            self.frame_time = now;
        }

        frame_deadline
    }
}
