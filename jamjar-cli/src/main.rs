use std::path::PathBuf;

use structopt::StructOpt;

use jamjar_cli::{PackageConfig, WebBuildConfig};

#[derive(StructOpt)]
struct PackageCmd {
    #[structopt(help = "The path to the root of your app. Defaults to current directory.")]
    #[structopt(parse(from_os_str))]
    app_root: Option<PathBuf>,

    #[structopt(
        long = "name",
        short = "n",
        help = "The name of the app. Defaults to the value in Cargo.toml."
    )]
    app_name: Option<String>,

    #[structopt(
        long = "output_dir",
        short = "o",
        help = "The directory to put the packaged archive into.",
        default_value = "./jamjar_build"
    )]
    #[structopt(parse(from_os_str))]
    output_dir: PathBuf,

    #[structopt(
        long = "icon_path",
        short = "i",
        help = "The icon image to use for the app. Defaults to `icon.png` in the app root."
    )]
    #[structopt(parse(from_os_str))]
    icon_path: Option<PathBuf>,

    #[structopt(
        long = "features",
        help = "Space-separated list of features to activate."
    )]
    features: Vec<String>,
}

#[derive(StructOpt)]
struct WebBuildCmd {
    #[structopt(help = "The path to the root of your app. Defaults to current directory.")]
    #[structopt(parse(from_os_str))]
    app_root: Option<PathBuf>,

    #[structopt(
        long = "bin_name",
        short = "b",
        help = "The name of the binary to build. Defaults to the project's default."
    )]
    bin_name: Option<String>,

    #[structopt(
        long = "output_dir",
        short = "o",
        help = "The directory to put the packaged archive into.",
        default_value = "./jamjar_web_build"
    )]
    #[structopt(parse(from_os_str))]
    output_dir: PathBuf,

    #[structopt(
        long = "features",
        help = "Space-separated list of features to activate."
    )]
    features: Vec<String>,
}

#[derive(StructOpt)]
enum JamjarCommand {
    Package(PackageCmd),
    WebBuild(WebBuildCmd),
}

fn main() {
    let cmd = JamjarCommand::from_args();
    match cmd {
        JamjarCommand::Package(build_cmd) => package(build_cmd),
        JamjarCommand::WebBuild(web_build_cmd) => web_build(web_build_cmd),
    }
}

fn package(build_cmd: PackageCmd) {
    let PackageCmd {
        app_root,
        app_name,
        output_dir,
        icon_path,
        features,
    } = build_cmd;

    let config = PackageConfig {
        app_root,
        app_name,
        output_dir,
        icon_path,
        features,
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
        bin_name,
        output_dir,
        features,
    } = web_build_cmd;

    let config = WebBuildConfig {
        app_root,
        bin_name,
        output_dir,
        features,
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
