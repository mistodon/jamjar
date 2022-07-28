#[cfg(feature = "codegen")]
pub mod codegen;

#[cfg(feature = "timing")]
pub mod anim;

pub mod atlas;

#[cfg(feature = "audio")]
pub mod audio;

pub mod color;

pub mod draw;

#[cfg(feature = "font")]
pub mod font;

#[cfg(feature = "gfx")]
pub mod gfx;

#[cfg(feature = "input")]
pub mod input;

pub mod layout;

#[cfg(feature = "logging")]
pub mod logging;

pub mod liveloop;

#[cfg(feature = "math")]
pub mod math;

pub mod menus;

#[cfg(feature = "mesh")]
pub mod mesh;

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

#[cfg(feature = "timing")]
pub mod timing;

pub mod utils;

#[cfg(feature = "windowing")]
pub mod web;

#[cfg(feature = "windowing")]
pub mod windowing;

#[cfg(target_arch = "wasm32")]
pub use web_sys;
