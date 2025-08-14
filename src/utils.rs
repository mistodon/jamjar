pub fn over<I: IntoIterator>(collection: I) -> I::IntoIter {
    collection.into_iter()
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum Flux<T> {
    Value(T),
    Change(T, T),
}

impl<T> Flux<T> {
    pub const fn new(t: T) -> Self {
        Flux::Value(t)
    }

    pub const fn change(a: T, b: T) -> Self {
        Flux::Change(a, b)
    }

    pub fn set(&mut self, x: T) {
        *self = Flux::Value(x);
    }

    pub const fn is_value(&self) -> bool {
        match self {
            Flux::Value(_) => true,
            Flux::Change(..) => false,
        }
    }

    pub const fn is_changing(&self) -> bool {
        match self {
            Flux::Value(_) => true,
            Flux::Change(..) => false,
        }
    }

    pub fn value(self) -> Option<T> {
        match self {
            Flux::Value(x) => Some(x),
            Flux::Change(..) => None,
        }
    }

    pub fn value_or<F: Fn(T, T) -> T>(self, f: F) -> T {
        match self {
            Flux::Value(x) => x,
            Flux::Change(a, b) => f(a, b),
        }
    }

    pub fn cancelled(self) -> T {
        match self {
            Flux::Value(x) => x,
            Flux::Change(a, _) => a,
        }
    }

    pub fn completed(self) -> T {
        match self {
            Flux::Value(x) => x,
            Flux::Change(_, b) => b,
        }
    }
}

impl<T: Clone> Flux<T> {
    pub fn complete(&mut self) {
        *self = Flux::Value(self.clone().completed());
    }

    pub fn cancel(&mut self) {
        *self = Flux::Value(self.clone().cancelled());
    }

    pub fn change_to(&mut self, b: T) {
        *self = Flux::Change(self.clone().completed(), b);
    }

    pub fn cancel_to(&mut self, b: T) {
        *self = Flux::Change(self.clone().cancelled(), b);
    }
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct Flag(bool);

impl Flag {
    pub const fn new(state: bool) -> Flag {
        Flag(state)
    }

    pub fn set_to(&mut self, x: bool) {
        self.0 = x;
    }

    pub fn set(&mut self) {
        self.0 = true;
    }

    pub fn clear(&mut self) {
        self.0 = false;
    }

    pub fn check(&mut self) -> bool {
        std::mem::replace(&mut self.0, false)
    }

    pub const fn peek(&self) -> bool {
        self.0
    }
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct IdPool {
    pub next_id: usize,
}

impl IdPool {
    pub fn new() -> Self {
        IdPool {
            next_id: 0,
        }
    }

    pub fn draw(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn clear(&mut self) {
        self.next_id = 0;
    }

    pub fn free_after(&mut self, max_id: usize) {
        self.next_id = std::cmp::min(self.next_id, max_id + 1);
    }

    pub fn free_unused(&mut self, used: impl Iterator<Item=usize>) {
        self.free_after(used.max().unwrap_or(std::usize::MAX));
    }
}
