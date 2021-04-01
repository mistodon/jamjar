use std::array::IntoIter;

pub fn over<T, const N: usize>(array: [T; N]) -> IntoIter<T, N> {
    IntoIter::new(array)
}
