pub use dirty_static::DirtyStatic;
pub use serde_yaml::from_str as parse_yaml;
pub use toml::from_str as parse_toml;

#[macro_export]
macro_rules! static_data_mod {
    ($visibility:vis mod $modname:ident {
        $(static $constname:ident : $datatype:ty = $fnname:ident ( $path:literal ) ;)*
        $(use static $constname2:ident : $datatype2:ty = $fnname2:ident ;)*
    }) => {

        $visibility mod $modname {
            use super::*;

            $(
                fn $fnname() -> Result<$datatype, ()> {
                    if $path.ends_with(".toml") {
                        jamjar::reloading::parse_toml(&jamjar::resource_str!($path)).map_err(|e| eprintln!("Failed to load {}: {}", stringify!($constname), e))
                    } else {
                        jamjar::reloading::parse_yaml(&jamjar::resource_str!($path)).map_err(|e| eprintln!("Failed to load {}: {}", stringify!($constname), e))
                    }
                }
            )*

            jamjar::lazy_static! {
                $(
                    pub static ref $constname: jamjar::reloading::DirtyStatic<$datatype> = jamjar::reloading::DirtyStatic::new($fnname().unwrap());
                )*
                $(
                    pub static ref $constname2: jamjar::reloading::DirtyStatic<$datatype2> = jamjar::reloading::DirtyStatic::new($fnname2().unwrap());
                )*
            }

            pub unsafe fn reload_all() {
                $(
                    $fnname().map(|x| $constname.replace(x)).unwrap_or(());
                )*
                $(
                    $fnname2().map(|x| $constname2.replace(x)).unwrap_or(());
                )*
            }
        }

    }
}
