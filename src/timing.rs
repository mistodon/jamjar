#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use std::marker::PhantomData;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = r#"
export function _system_secs_f64() {
  return performance.now() / 1000.0;
}"#)]
extern "C" {
    fn _system_secs_f64() -> f64;
}

#[cfg(not(target_arch = "wasm32"))]
fn _system_secs_f64() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

#[derive(Debug, Clone, Copy, PartialOrd, PartialEq)]
pub struct LogicTime;

#[derive(Debug, Clone, Copy, PartialOrd, PartialEq)]
pub struct RealTime;

#[derive(Debug, Clone, Copy, PartialOrd, PartialEq)]
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

    pub fn secs(self) -> f64 {
        self.0
    }

    pub fn minus(self, amount: f64) -> Self {
        Timestamp(self.0 - amount, self.1)
    }

    pub fn plus(self, amount: f64) -> Self {
        Timestamp(self.0 + amount, self.1)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Clock<T>(f64, PhantomData<T>);

impl<T> Clock<T> {
    pub fn new_zero() -> Self {
        Clock(0.0, PhantomData)
    }

    pub fn new_now() -> Self {
        Clock(_system_secs_f64(), PhantomData)
    }

    pub fn secs(&self) -> f64 {
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

    pub fn now(&self) -> Timestamp<T> {
        Timestamp(self.0, self.1)
    }

    pub fn since(&self, time: Timestamp<T>) -> f64 {
        self.0 - time.0
    }

    pub fn until(&self, time: Timestamp<T>) -> f64 {
        -self.since(time)
    }
}

pub type RealClock = Clock<RealTime>;
pub type RealTimestamp = Timestamp<RealTime>;
