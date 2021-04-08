pub use log;

pub fn init_logging() {
    #[cfg(target_arch = "wasm32")]
    {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
        console_log::init_with_level(log::Level::Debug).unwrap();
    }
}

#[macro_export]
macro_rules! jprintln {
    () => {
        if cfg!(target_arch = "wasm32") { $crate::logging::log::info!() } else { eprintln!() }
    };
    ($($arg:tt)*) => {
        if cfg!(target_arch = "wasm32") { $crate::logging::log::info!($($arg)*) } else { eprintln!($($arg)*) }
    }
}

#[macro_export]
macro_rules! jprint {
    () => {
        if cfg!(target_arch = "wasm32") { $crate::logging::log::info!() } else { eprint!() }
    };
    ($($arg:tt)*) => {
        if cfg!(target_arch = "wasm32") { $crate::logging::log::info!($($arg)*) } else { eprint!($($arg)*) }
    }
}

#[macro_export]
macro_rules! dprintln {
    () => {
        if cfg!(debug_assertions) { jprintln!() } else { () }
    };
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) { jprintln!($($arg)*) } else { () }
    }
}

#[macro_export]
macro_rules! dprint {
    () => {
        if cfg!(debug_assertions) { jprint!() } else { () }
    };
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) { jprint!($($arg)*) } else { () }
    }
}
