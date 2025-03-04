use cfg_aliases::cfg_aliases;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    cfg_aliases! {
        android_platform: { target_os = "android" },
        web_platform: { all(target_family = "wasm", target_os = "unknown") },
        macos_platform: { target_os = "macos" },
        ios_platform: { all(target_vendor = "apple", not(target_os = "macos")) },
        windows_platform: { target_os = "windows" },
        free_unix: { all(unix, not(target_vendor = "apple"), not(android_platform), not(target_os = "emscripten")) },
        redox: { target_os = "redox" },
    }
}
