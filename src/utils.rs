pub fn over<I: IntoIterator>(collection: I) -> I::IntoIter {
    collection.into_iter()
}
