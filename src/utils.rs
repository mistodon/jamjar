use std::array::IntoIter;

pub fn over<T, const N: usize>(array: [T; N]) -> IntoIter<T, N> {
    IntoIter::new(array)
}

pub trait ArrayIter {
    type Item;
    type IntoIter: Iterator<Item = Self::Item>;

    fn array_iter(self) -> Self::IntoIter;
}

impl<T> ArrayIter for Vec<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn array_iter(self) -> Self::IntoIter {
        self.into_iter()
    }
}

impl<T, const N: usize> ArrayIter for [T; N] {
    type Item = T;
    type IntoIter = std::array::IntoIter<Self::Item, N>;

    fn array_iter(self) -> Self::IntoIter {
        std::array::IntoIter::new(self)
    }
}

impl<'a, T: Copy> ArrayIter for &'a [T] {
    type Item = T;
    type IntoIter = std::iter::Copied<std::slice::Iter<'a, T>>;

    fn array_iter(self) -> Self::IntoIter {
        self.iter().copied()
    }
}
