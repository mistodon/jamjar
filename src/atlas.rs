#[cfg(feature = "font")]
pub mod font;

#[cfg(feature = "image_atlas")]
pub mod image;

#[cfg(feature = "image_atlas")]
pub mod image_array;

#[cfg(all(feature = "image_atlas", feature = "font"))]
mod font_image;

#[cfg(feature = "mesh")]
pub mod mesh;

#[cfg(all(feature = "image_atlas", feature = "font"))]
pub use self::font_image::*;

pub trait Atlas<Insert, Key: ?Sized, Fetch, Storage, Updated> {
    fn insert(&mut self, insertion: Insert);
    fn fetch(&self, key: &Key) -> Fetch;
    fn remove_and_invalidate(&mut self, key: &Key);
    fn compile_into(&mut self, dest: &mut Storage) -> Option<Updated>;
    fn modified(&self) -> bool;
}
