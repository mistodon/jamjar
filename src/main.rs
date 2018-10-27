extern crate jamjar;

#[macro_use]
extern crate structopt_derive;

extern crate failure;
extern crate structopt;

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
}

fn main() {
    let JamjarCommand {
        app_root,
        app_name,
        output_dir,
    } = JamjarCommand::from_args();

    let config = Configuration {
        app_root,
        app_name,
        output_dir,
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