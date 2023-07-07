use std::io::Error as IOError;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use handlebars::{Handlebars, RenderError};
use image::ImageError;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use toml::de::Error as TomlError;
use zip::{
    result::ZipError,
    write::{FileOptions, ZipWriter},
};

#[derive(Debug, Error)]
pub enum JamjarError {
    #[error("an IO error occurred")]
    IOError(#[from] IOError),

    #[error("an IO error occurred: {message}\n{cause}")]
    IOContextError { cause: IOError, message: String },

    #[error("an error occurred while parsing TOML file")]
    TomlError {
        #[from]
        cause: TomlError,
    },

    #[error("an error occurred while writing to template")]
    TemplateError {
        #[from]
        cause: RenderError,
    },

    #[error("failed to decode icon image")]
    ImageError(#[from] ImageError),

    #[error("external command `{0}` failed")]
    ExternalCommandError(&'static str),

    #[error("an error occurred while compressing data")]
    ZipError(#[from] ZipError),

    #[error("an error occurred: {0}")]
    StringError(String),
}

impl JamjarError {
    fn io(cause: IOError, message: &str) -> Self {
        JamjarError::IOContextError {
            cause,
            message: message.into(),
        }
    }
}

#[derive(Debug)]
pub struct PackageConfig {
    pub app_root: Option<PathBuf>,
    pub app_name: Option<String>,
    pub output_dir: PathBuf,
    pub icon_path: Option<PathBuf>,
    pub features: Vec<String>,
    pub profile: String,
}

#[derive(Debug)]
pub struct WebBuildConfig {
    pub app_root: Option<PathBuf>,
    pub app_name: Option<String>,
    pub bin_name: Option<String>,
    pub example: Option<String>,
    pub output_dir: PathBuf,
    pub web_includes: PathBuf,
    pub features: Vec<String>,
    pub profile: String,
    pub include_spirv_cross: bool,
}

struct AppConfig<'a> {
    app_root: &'a Path,
    app_name: &'a str,
    exe_name: &'a str,
    version: &'a str,
    bundle_id: &'a str,
    icon_path: &'a Path,
    target_name: &'a str,
}

#[derive(Debug, Deserialize)]
struct CargoManifest {
    package: CargoManifestPackage,
}

#[derive(Debug, Deserialize)]
struct CargoManifestPackage {
    name: String,
    version: String,
}

pub fn package_app(config: &PackageConfig) -> Result<PathBuf, JamjarError> {
    use std::fs::File;

    let cwd = match config.app_root {
        Some(ref path) => path.canonicalize().map_err(|e| {
            JamjarError::io(
                e,
                &format!(
                    "The input directory '{}' could not be found.",
                    path.display()
                ),
            )
        })?,
        None => std::env::current_dir()
            .map_err(|e| JamjarError::io(e, "Failed to get current directory."))?,
    };

    println!("App is at: {}", cwd.display());

    let profile = &config.profile;
    println!("Compiling app for {profile}:");
    {
        let mut cmd = Command::new("cargo");
        cmd.current_dir(&cwd)
            .arg("build")
            .arg("--profile")
            .arg(profile);

        if !config.features.is_empty() {
            cmd.arg("--no-default-features");
            cmd.arg("--features");
            cmd.args(config.features.iter());
        }
        cmd.stdout(Stdio::inherit());

        let output = cmd.output()?;

        print!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));

        if !output.status.success() {
            return Err(JamjarError::ExternalCommandError("cargo"));
        }
    }

    let manifest_toml = {
        let manifest_path = cwd.join("Cargo.toml");
        std::fs::read_to_string(&manifest_path)
            .map_err(|e| JamjarError::io(e, "Could not read Cargo.toml."))?
    };

    let manifest = toml::from_str::<CargoManifest>(&manifest_toml)
        .map_err(|e| JamjarError::TomlError { cause: e })?;

    let app_name = config
        .app_name
        .to_owned()
        .unwrap_or_else(|| manifest.package.name.clone());
    let exe_name = manifest.package.name;

    let icon_path = match config.icon_path {
        Some(ref path) => path.to_owned(),
        None => cwd.join("icon.png"),
    };

    println!(
        "App name is: {}\nVersion is: {}\nIcon path is: {}",
        app_name,
        manifest.package.version,
        icon_path.display(),
    );

    std::fs::create_dir_all(&config.output_dir)
        .map_err(|e| JamjarError::io(e, "Failed to create output directory."))?;

    let platform = {
        #[cfg(windows)]
        {
            "win"
        }
        #[cfg(target_os = "macos")]
        {
            "macos"
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            "linux"
        }
    };

    let output_path = config.output_dir.join(format!(
        "{}_{}_{}.zip",
        app_name, platform, manifest.package.version
    ));

    let temp_dir = tempfile::tempdir()
        .map_err(|e| JamjarError::io(e, "Failed to create temporary directory."))?;

    println!("Creating macOS app");

    let app_config = AppConfig {
        app_root: &cwd,
        app_name: &app_name,
        exe_name: &exe_name,
        version: &manifest.package.version,
        bundle_id: &app_name,
        icon_path: &icon_path,
        target_name: if profile == "dev" { "debug" } else { profile },
    };

    let _app_path = create_macos_app(&app_config, temp_dir.as_ref())?;

    println!("Compressing app to output");
    let mut output_file = File::create(&output_path)
        .map_err(|e| JamjarError::io(e, "Failed to create output file."))?;

    let mut zipper = ZipWriter::new(&mut output_file);
    let mut dirs = vec![temp_dir.as_ref().to_owned()];

    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir)? {
            use std::io::Write;

            let entry = entry?;
            let path = entry.path();

            if entry.file_type()?.is_file() {
                let rel_path = path.strip_prefix(&temp_dir).unwrap().to_owned();
                zipper.start_file(
                    rel_path.to_string_lossy(),
                    FileOptions::default().unix_permissions(0o755),
                )?;
                let contents = std::fs::read(path)?;
                zipper.write_all(&contents)?;
            } else {
                dirs.push(path);
            }
        }
    }

    zipper.finish()?;

    Ok(output_path)
}

fn create_macos_app(config: &AppConfig, destination: &Path) -> Result<PathBuf, JamjarError> {
    use std::os::unix::fs::PermissionsExt;

    let AppConfig {
        app_root,
        app_name,
        exe_name,
        version,
        bundle_id,
        icon_path,
        target_name,
    } = config;

    let app_path = destination.join(format!("{}.app", app_name));
    let contents_path = app_path.join("Contents");
    let macos_path = contents_path.join("MacOS");
    let resources_path = contents_path.join("Resources");
    let plist_path = contents_path.join("Info.plist");
    let app_exe_path = macos_path.join(app_name);
    let app_icons_path = resources_path.join("Icon.icns");

    std::fs::create_dir_all(&macos_path)?;
    std::fs::create_dir_all(&resources_path)?;
    std::fs::create_dir_all(&contents_path)?;

    // Info.plist
    #[derive(Serialize)]
    struct InfoPlist<'a> {
        app_name: &'a str,
        version: &'a str,
        bundle_id: &'a str,
    }

    let template = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/Info.plist"));
    let context = InfoPlist {
        app_name,
        version,
        bundle_id,
    };

    let hb = Handlebars::new();
    let info_plist = hb
        .render_template(&template, &context)
        .map_err(|e| JamjarError::TemplateError { cause: e })?;

    std::fs::write(&plist_path, &info_plist)
        .map_err(|e| JamjarError::io(e, "Failed to write Info.plist."))?;

    // Icons
    {
        println!("Creating icon set:");

        let temp_icons_dir = tempfile::tempdir()?;
        let temp_icons_dir = temp_icons_dir
            .as_ref()
            .join(format!("{}.iconset", app_name));
        std::fs::create_dir(&temp_icons_dir)?;

        let image_bytes = std::fs::read(icon_path)?;
        let image = image::load_from_memory(&image_bytes)?;

        let sizes = &[
            ((16, 16), "icon_16x16.png"),
            ((32, 32), "icon_16x16@2x.png"),
            ((32, 32), "icon_32x32.png"),
            ((64, 64), "icon_32x32@2x.png"),
            ((128, 128), "icon_128x128.png"),
            ((256, 256), "icon_128x128@2x.png"),
            ((256, 256), "icon_256x256.png"),
            ((512, 512), "icon_256x256@2x.png"),
            ((512, 512), "icon_512x512.png"),
            ((1024, 1024), "icon_512x512@2x.png"),
        ];

        for &((width, height), filename) in sizes {
            use image::imageops::FilterType;

            let resized_image = image.resize_exact(width, height, FilterType::CatmullRom);
            resized_image.save(temp_icons_dir.join(filename))?;
            println!("  Resized to {}", filename);
        }

        println!("Running iconutil");
        let output = Command::new("iconutil")
            .arg("-c")
            .arg("icns")
            .arg(temp_icons_dir)
            .arg("--output")
            .arg(&app_icons_path)
            .stdout(Stdio::inherit())
            .output()?;

        print!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));

        if !output.status.success() {
            return Err(JamjarError::ExternalCommandError("iconutil"));
        }
    }

    // Executable
    let exe_path = app_root.join(format!("target/{target_name}/{exe_name}"));
    std::fs::copy(&exe_path, &app_exe_path)?;

    let mut perms = std::fs::metadata(&app_exe_path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&app_exe_path, perms)?;

    Ok(app_path)
}

pub fn web_build(config: &WebBuildConfig) -> Result<PathBuf, JamjarError> {
    let cwd = match config.app_root {
        Some(ref path) => path.canonicalize().map_err(|e| {
            JamjarError::io(
                e,
                &format!(
                    "The input directory '{}' could not be found.",
                    path.display()
                ),
            )
        })?,
        None => std::env::current_dir()
            .map_err(|e| JamjarError::io(e, "Failed to get current directory."))?,
    };

    let manifest_toml = {
        let manifest_path = cwd.join("Cargo.toml");
        std::fs::read_to_string(&manifest_path)
            .map_err(|e| JamjarError::io(e, "Could not read Cargo.toml."))?
    };

    let manifest = toml::from_str::<CargoManifest>(&manifest_toml)
        .map_err(|e| JamjarError::TomlError { cause: e })?;

    let app_name = config
        .app_name
        .to_owned()
        .unwrap_or_else(|| manifest.package.name.clone());

    let final_bin_name = config
        .example
        .as_ref()
        .unwrap_or(config.bin_name.as_ref().unwrap_or(&manifest.package.name));

    std::fs::create_dir_all(&config.output_dir)
        .map_err(|e| JamjarError::io(e, "Failed to create output directory."))?;

    let profile = &config.profile;
    println!("Compiling app for {profile}:");
    {
        let mut cmd = Command::new("cargo");
        cmd.current_dir(&cwd)
            .env("RUSTFLAGS", "--cfg=web_sys_unstable_apis")
            .arg("build")
            .args(&["--color", "always"])
            .arg("--profile")
            .arg(profile)
            .arg("--target")
            .arg("wasm32-unknown-unknown");

        if let Some(bin_name) = &config.bin_name {
            cmd.arg("--bin");
            cmd.arg(bin_name);
        } else if let Some(example) = &config.example {
            cmd.arg("--example");
            cmd.arg(example);
        }

        if !config.features.is_empty() {
            cmd.arg("--no-default-features");
            cmd.arg("--features");
            cmd.args(config.features.iter());
        }

        let mut child = cmd.spawn()?;
        let status = child.wait()?;

        if !status.success() {
            return Err(JamjarError::ExternalCommandError("cargo"));
        }
    }

    println!("Running wasm-bindgen:");
    {
        let mut wasm_path = cwd.clone();
        wasm_path.push("target");
        wasm_path.push("wasm32-unknown-unknown");
        wasm_path.push(if profile == "dev" { "debug" } else { profile });
        if config.example.is_some() {
            wasm_path.push("examples");
        }
        wasm_path.push(format!("{}.wasm", &final_bin_name));

        let mut cmd = Command::new("wasm-bindgen");
        cmd.current_dir(&cwd)
            .arg(wasm_path)
            .arg("--out-dir")
            .arg(&config.output_dir)
            .arg("--web");
        cmd.stdout(Stdio::inherit());

        let output = cmd.output()?;

        print!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));

        if !output.status.success() {
            return Err(JamjarError::ExternalCommandError("cargo"));
        }
    }

    println!("Creating index.html:");
    {
        // index.html
        #[derive(Serialize)]
        struct IndexHtml<'a> {
            app_name: &'a str,
            bin_name: &'a str,
            inline_js: &'a str,
            inline_css: &'a str,
            onload_js: &'a str,
        }

        let no_spirv_template =
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/index.html"));
        let spirv_template = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/templates/index_spirv.html"
        ));
        let template = if config.include_spirv_cross {
            spirv_template
        } else {
            no_spirv_template
        };

        let (inline_js, inline_css, onload_js) = {
            let path = config.web_includes.clone();
            let mut inline_js_path = path.clone();
            inline_js_path.push("inline.js");
            let mut inline_css_path = path.clone();
            inline_css_path.push("inline.css");
            let mut onload_js_path = path.clone();
            onload_js_path.push("onload.js");

            let inline_js =
                std::fs::read_to_string(&inline_js_path).unwrap_or_else(|_| String::new());
            let inline_css =
                std::fs::read_to_string(&inline_css_path).unwrap_or_else(|_| String::new());
            let onload_js =
                std::fs::read_to_string(&onload_js_path).unwrap_or_else(|_| String::new());

            (inline_js, inline_css, onload_js)
        };

        let context = IndexHtml {
            app_name: &app_name,
            bin_name: &final_bin_name,
            inline_js: &inline_js,
            inline_css: &inline_css,
            onload_js: &onload_js,
        };

        let mut hb = Handlebars::new();
        hb.register_escape_fn(handlebars::no_escape);

        let html = hb
            .render_template(&template, &context)
            .map_err(|e| JamjarError::TemplateError { cause: e })?;

        let mut index_path = config.output_dir.clone();
        index_path.push("index.html");

        std::fs::write(&index_path, &html)
            .map_err(|e| JamjarError::io(e, "Failed to write index.html"))?;
    }

    if config.include_spirv_cross {
        println!("Copying spirv_cross scripts:");

        let spirv_js = include_str!("../ext/spirv_cross/spirv_cross_wrapper_glsl.js");
        let spirv_wasm = include_bytes!("../ext/spirv_cross/spirv_cross_wrapper_glsl.wasm");

        let mut js_path = config.output_dir.clone();
        js_path.push("spirv_cross_wrapper_glsl.js");

        let mut wasm_path = config.output_dir.clone();
        wasm_path.push("spirv_cross_wrapper_glsl.wasm");

        std::fs::write(&js_path, spirv_js)?;
        std::fs::write(&wasm_path, spirv_wasm)?;
    }

    Ok(config.output_dir.clone())
}
