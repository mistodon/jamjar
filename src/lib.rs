#[cfg(feature = "codegen")]
pub mod codegen;

#[cfg(feature = "reloading")]
pub mod reloading;
#[cfg(feature = "reloading")]
pub use dymod::dymod;
#[cfg(feature = "reloading")]
pub use lazy_static::lazy_static;
#[cfg(feature = "reloading")]
pub use resource::resource_str;

#[cfg(feature = "windowing")]
pub mod windowing;
