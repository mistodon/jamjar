#[cfg(feature = "codegen")]
pub mod codegen;

#[cfg(feature = "audio")]
pub mod audio;

#[cfg(feature = "drawsloth")]
pub mod drawsloth;

#[cfg(feature = "gfx")]
pub mod gfx;

#[cfg(feature = "reloading")]
pub mod reloading;
#[cfg(feature = "reloading")]
pub use dymod::dymod;
#[cfg(feature = "reloading")]
pub use lazy_static::lazy_static;
#[cfg(any(feature = "reloading", feature = "resources"))]
pub use resource::*;

#[cfg(feature = "resources")]
pub mod resources;

#[cfg(feature = "windowing")]
pub mod windowing;
