// Anim<T> =
//     start time
//     duration
//     loop? pingpong??
//     data: T (only stored, user decides what to do with it)

//     methods:
//         t()
//         inv_t()
//         restart(new_data)
//         invert(new_data) // e.g. 0.25 becomes 0.75

use crate::timing::{LogicTime, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LoopType {
    Once,
    Loop,
    PingPong,
}

impl Default for LoopType {
    fn default() -> Self {
        LoopType::Once
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Anim<T, Clock: Copy = LogicTime> {
    pub data: T,
    pub start: Timestamp<Clock>,
    pub duration: f64,
    pub loop_type: LoopType,
}

impl<T, Clock: Copy> Anim<T, Clock> {
    pub fn new(data: T, start: Timestamp<Clock>, duration: f64) -> Self {
        Anim {
            data,
            start,
            duration,
            loop_type: LoopType::Once,
        }
    }

    pub fn looped(data: T, start: Timestamp<Clock>, duration: f64) -> Self {
        Anim {
            data,
            start,
            duration,
            loop_type: LoopType::Loop,
        }
    }

    pub fn pingpong(data: T, start: Timestamp<Clock>, duration: f64) -> Self {
        Anim {
            data,
            start,
            duration,
            loop_type: LoopType::PingPong,
        }
    }

    pub fn at(&self, time: Timestamp<Clock>) -> Moment<T, Clock> {
        Moment { anim: self, time }
    }

    pub fn at_mut(&mut self, time: Timestamp<Clock>) -> MomentMut<T, Clock> {
        MomentMut { anim: self, time }
    }

    pub fn then(&self, data: T, duration: f64) -> Self {
        Anim::new(data, self.start.plus(self.duration), duration)
    }

    pub fn then_loop(&self, data: T, duration: f64) -> Self {
        Anim::looped(data, self.start.plus(self.duration), duration)
    }

    pub fn then_pingpong(&self, data: T, duration: f64) -> Self {
        Anim::pingpong(data, self.start.plus(self.duration), duration)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Moment<'a, T, Clock: Copy = LogicTime> {
    pub anim: &'a Anim<T, Clock>,
    pub time: Timestamp<Clock>,
}

impl<'a, T, Clock: Copy> Moment<'a, T, Clock> {
    pub fn time(&self) -> f64 {
        self.time.since(self.anim.start)
    }

    pub fn time_left(&self) -> f64 {
        self.anim.duration - self.time()
    }

    pub fn started(&self) -> bool {
        self.time() >= 0.
    }

    pub fn finished(&self) -> bool {
        self.anim.loop_type == LoopType::Once && self.time_left() <= 0.
    }

    pub fn active(&self) -> bool {
        self.started() && !self.finished()
    }

    pub fn pre_t(&self) -> f64 {
        match self.anim.loop_type {
            LoopType::Once => (self.time() / self.anim.duration).min(1.),
            LoopType::Loop => self.time() / self.anim.duration,
            LoopType::PingPong => {
                let t = self.time() / self.anim.duration;
                let c = t as usize;
                if c % 2 == 0 {
                    t.fract()
                } else {
                    1. - t.fract()
                }
            }
        }
    }

    pub fn t(&self) -> f64 {
        self.pre_t().max(0.)
    }

    pub fn inv_t(&self) -> f64 {
        1. - self.t()
    }

    pub fn ease_pre_t<F: Fn(f64) -> f64>(&self, f: F) -> f64 {
        f(self.pre_t())
    }

    pub fn ease_t<F: Fn(f64) -> f64>(&self, f: F) -> f64 {
        f(self.t())
    }

    pub fn ease_inv_t<F: Fn(f64) -> f64>(&self, f: F) -> f64 {
        f(self.inv_t())
    }
}

#[derive(Debug, PartialEq)]
pub struct MomentMut<'a, T, Clock: Copy = LogicTime> {
    pub anim: &'a mut Anim<T, Clock>,
    pub time: Timestamp<Clock>,
}

impl<'a, T, Clock: Copy> MomentMut<'a, T, Clock> {
    fn imm(&'a self) -> Moment<'a, T, Clock> {
        Moment {
            anim: self.anim,
            time: self.time,
        }
    }

    pub fn time(&self) -> f64 {
        self.imm().time()
    }

    pub fn time_left(&self) -> f64 {
        self.imm().time_left()
    }

    pub fn started(&self) -> bool {
        self.imm().started()
    }

    pub fn finished(&self) -> bool {
        self.imm().finished()
    }

    pub fn active(&self) -> bool {
        self.imm().active()
    }

    pub fn pre_t(&self) -> f64 {
        self.imm().pre_t()
    }

    pub fn t(&self) -> f64 {
        self.imm().t()
    }

    pub fn inv_t(&self) -> f64 {
        self.imm().inv_t()
    }

    pub fn ease_pre_t<F: Fn(f64) -> f64>(&self, f: F) -> f64 {
        self.imm().ease_pre_t(f)
    }

    pub fn ease_t<F: Fn(f64) -> f64>(&self, f: F) -> f64 {
        self.imm().ease_t(f)
    }

    pub fn ease_inv_t<F: Fn(f64) -> f64>(&self, f: F) -> f64 {
        self.imm().ease_inv_t(f)
    }

    pub fn reset(&mut self) {
        self.anim.start = self.time;
    }

    pub fn restart(&mut self, data: T) {
        self.anim.start = self.time;
        self.anim.data = data;
    }

    pub fn replace(&mut self, data: T, duration: f64) {
        self.anim.start = self.time;
        self.anim.data = data;
        self.anim.duration = duration;
    }

    pub fn invert(&mut self, data: T) {
        let time_elapsed = self.time_left().max(0.);
        let start_time = self.time.minus(time_elapsed);
        self.anim.start = start_time;
        self.anim.data = data;
    }
}
