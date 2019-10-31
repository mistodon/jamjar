extern crate jamjar;

extern crate failure;
extern crate structopt;
extern crate structopt_derive;

use std::path::PathBuf;

use structopt::StructOpt;

use jamjar::Configuration;

#[derive(StructOpt)]
struct JamjarCommand {
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

fn main() {
    let JamjarCommand {
        app_root,
        app_name,
        output_dir,
        icon_path,
        features,
    } = JamjarCommand::from_args();

    let config = Configuration {
        app_root,
        app_name,
        output_dir,
        icon_path,
        features,
    };

    match jamjar::package_app(&config) {
        Ok(path) => {
            println!("Release created at: {}", path.display());
        }
        Err(e) => {
            eprintln!("Packaging failed: {}", e);
            std::process::exit(1);
        }
    }
}
