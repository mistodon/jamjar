#[cfg(feature = "font")]
pub mod font;

#[cfg(feature = "image_atlas")]
pub mod image;

#[cfg(all(feature = "image_atlas", feature = "font"))]
mod font_image;

#[cfg(all(feature = "image_atlas", feature = "font"))]
pub use self::font_image::*;

pub trait Atlas<Insert, Key: ?Sized, Fetch, Storage> {
    fn insert(&mut self, insertion: Insert);
    fn fetch(&self, key: &Key) -> Fetch;
    fn compile_into(&mut self, dest: &mut Storage) -> bool;
    fn modified(&self) -> bool;
}
