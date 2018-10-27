#[macro_use]
extern crate failure;
#[macro_use]
extern crate serde_derive;

extern crate handlebars;
extern crate serde;
extern crate tempfile;
extern crate toml;
extern crate zip;

use std::io::Error as IOError;
use std::path::{Path, PathBuf};

use handlebars::{Handlebars, TemplateRenderError};
use toml::de::Error as TomlError;
use zip::{
    result::ZipError,
    write::{FileOptions, ZipWriter},
};

#[derive(Debug, Fail)]
pub enum JamjarError {
    #[fail(display = "an IO error occurred: {}", message)]
    IOError {
        #[cause]
        cause: IOError,
        message: String,
    },

    #[fail(
        display = "an error occurred while parsing TOML file: {}",
        cause
    )]
    TomlError {
        #[cause]
        cause: TomlError,
    },

    #[fail(
        display = "an error occurred while writing to template: {}",
        cause
    )]
    TemplateError {
        #[cause]
        cause: TemplateRenderError,
    },

    #[fail(display = "project failed to build")]
    CargoError,

    #[fail(display = "an error occurred while compressing data: {}", _0)]
    ZipError(#[cause] ZipError),

    #[fail(display = "an error occurred: {}", _0)]
    StringError(String),
}

impl JamjarError {
    fn io(cause: IOError, message: &str) -> Self {
        JamjarError::IOError {
            cause,
            message: message.into(),
        }
    }
}

impl From<IOError> for JamjarError {
    fn from(cause: IOError) -> Self {
        let message = cause.to_string();
        JamjarError::IOError { cause, message }
    }
}

impl From<ZipError> for JamjarError {
    fn from(e: ZipError) -> Self {
        JamjarError::ZipError(e)
    }
}

#[derive(Debug)]
pub struct Configuration {
    pub app_root: Option<PathBuf>,
    pub app_name: Option<String>,
    pub output_dir: PathBuf,
}

struct AppConfig<'a> {
    app_root: &'a Path,
    app_name: &'a str,
    version: &'a str,
    bundle_id: &'a str,
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

pub fn package_app(config: &Configuration) -> Result<PathBuf, JamjarError> {
    use std::fs::File;
    use std::process::Command;

    let cwd = match config.app_root {
        Some(ref path) => path
            .canonicalize()
            .map_err(|e| JamjarError::io(e, &format!("The input directory '{}' could not be found.", path.display())))?,
        None => std::env::current_dir()
            .map_err(|e| JamjarError::io(e, "Failed to get current directory."))?,
    };

    println!("App is at: {}", cwd.display());

    println!("Compiling app for release:");
    {
        let output = Command::new("cargo")
            .current_dir(&cwd)
            .arg("build")
            .arg("--release")
            .output()?;

        print!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));

        if !output.status.success() {
            return Err(JamjarError::CargoError);
        }
    }

    let manifest_toml = {
        let manifest_path = cwd.join("Cargo.toml");
        std::fs::read_to_string(&manifest_path)
            .map_err(|e| JamjarError::io(e, "Could not read Cargo.toml."))?
    };

    let manifest = toml::from_str::<CargoManifest>(&manifest_toml)
        .map_err(|e| JamjarError::TomlError { cause: e })?;

    let app_name = config.app_name.to_owned().unwrap_or(manifest.package.name);

    println!(
        "App name is: {}\nVersion is: {}",
        app_name, manifest.package.version
    );

    std::fs::create_dir_all(&config.output_dir)
        .map_err(|e| JamjarError::io(e, "Failed to create output directory."))?;

    let output_path = config
        .output_dir
        .join(format!("{}_{}.zip", app_name, manifest.package.version));

    let temp_dir = tempfile::tempdir()
        .map_err(|e| JamjarError::io(e, "Failed to create temporary directory."))?;

    println!("Creating macOS app");

    let app_config = AppConfig {
        app_root: &cwd,
        app_name: &app_name,
        version: &manifest.package.version,
        bundle_id: &app_name,
    };

    let _app_path = create_macos_app(app_config, temp_dir.as_ref())?;

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

fn create_macos_app(config: AppConfig, destination: &Path) -> Result<PathBuf, JamjarError> {
    use std::os::unix::fs::PermissionsExt;

    let AppConfig {
        app_root,
        app_name,
        version,
        bundle_id,
    } = config;

    let app_path = destination.join(format!("{}.app", app_name));
    let contents_path = app_path.join("Contents");
    let macos_path = contents_path.join("MacOS");
    let resources_path = contents_path.join("Resources");
    let plist_path = contents_path.join("Info.plist");
    let app_exe_path = macos_path.join(app_name);

    std::fs::create_dir_all(&macos_path)?;
    std::fs::create_dir_all(&resources_path)?;
    std::fs::create_dir_all(&contents_path)?;

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

    let exe_path = app_root.join(format!("target/release/{}", app_name));
    std::fs::copy(&exe_path, &app_exe_path)?;

    let mut perms = std::fs::metadata(&app_exe_path)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&app_exe_path, perms)?;

    Ok(app_path)
}
