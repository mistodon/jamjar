use std::path::PathBuf;

use structopt::StructOpt;

use jamjar_cli::{PackageConfig, WebBuildConfig};

/// Package the app into an archive for distribution.
#[derive(StructOpt)]
struct PackageCmd {
    /// The path to the root of your app. Defaults to current directory.
    #[structopt(parse(from_os_str))]
    app_root: Option<PathBuf>,

    /// The name of the app. Defaults to the value in Cargo.toml
    #[structopt(long = "name", short = "n")]
    app_name: Option<String>,

    /// The directory to put the packaged archive into.
    #[structopt(long = "output_dir", short = "o", default_value = "./target/jamjar")]
    #[structopt(parse(from_os_str))]
    output_dir: PathBuf,

    /// The icon image to use for the app. Defaults to `icon.png` in the app root.
    #[structopt(long = "icon_path", short = "i")]
    #[structopt(parse(from_os_str))]
    icon_path: Option<PathBuf>,

    /// The profile to compile with.
    #[structopt(long = "profile", default_value = "release")]
    profile: String,

    /// Space-separated list of features to activate.
    #[structopt(long = "features")]
    features: Vec<String>,
}

/// Create a web build of the app for testing or distrubution.
#[derive(StructOpt)]
struct WebBuildCmd {
    /// The path to the root of your app. Defaults to current directory.
    #[structopt(parse(from_os_str))]
    app_root: Option<PathBuf>,

    /// The name of the app. Defaults to the value in Cargo.toml
    #[structopt(long = "name", short = "n")]
    app_name: Option<String>,

    /// The name of the binary to build. Defaults to the project's default.
    #[structopt(long = "bin_name", short = "b")]
    bin_name: Option<String>,

    /// The directory to put the packaged archive into.
    #[structopt(
        long = "output_dir",
        short = "o",
        default_value = "./target/jamjar_web"
    )]
    #[structopt(parse(from_os_str))]
    output_dir: PathBuf,

    /// Space-separated list of features to activate.
    #[structopt(long = "features")]
    features: Vec<String>,

    /// The profile to compile with.
    #[structopt(long = "profile", default_value = "release")]
    profile: String,

    #[structopt(long = "web-includes", short = "w", default_value = "./web")]
    web_includes: PathBuf,

    /// Use this flag to skip packaging spirv_cross scripts.
    #[structopt(long)]
    bypass_spirv_cross: bool,
}

/// A simple, opinionated tool for packaging Rust apps (mostly game jam games) for different platforms
#[derive(StructOpt)]
enum JamjarCommand {
    Package(PackageCmd),
    Web(WebBuildCmd),
}

fn main() {
    let cmd = JamjarCommand::from_args();
    match cmd {
        JamjarCommand::Package(build_cmd) => package(build_cmd),
        JamjarCommand::Web(web_build_cmd) => web_build(web_build_cmd),
    }
}

fn package(build_cmd: PackageCmd) {
    let PackageCmd {
        app_root,
        app_name,
        output_dir,
        icon_path,
        features,
        profile,
    } = build_cmd;

    let config = PackageConfig {
        app_root,
        app_name,
        output_dir,
        icon_path,
        features,
        profile,
    };

    match jamjar_cli::package_app(&config) {
        Ok(path) => {
            println!("Release created at: {}", path.display());
        }
        Err(e) => {
            eprintln!("Packaging failed: {}", e);
            std::process::exit(1);
        }
    }
}

fn web_build(web_build_cmd: WebBuildCmd) {
    let WebBuildCmd {
        app_root,
        app_name,
        bin_name,
        output_dir,
        features,
        profile,
        web_includes,
        bypass_spirv_cross,
    } = web_build_cmd;

    let config = WebBuildConfig {
        app_root,
        app_name,
        bin_name,
        output_dir,
        features,
        profile,
        web_includes,
        bypass_spirv_cross,
    };

    match jamjar_cli::web_build(&config) {
        Ok(path) => {
            println!("Built for web. Host here to test: {}", path.display());
        }
        Err(e) => {
            eprintln!("Packaging failed: {}", e);
            std::process::exit(1);
        }
    }
}
