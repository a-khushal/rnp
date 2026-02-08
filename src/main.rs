use clap::{Parser, Subcommand};

mod cache;
mod commands;
use commands::{
    init::handle_init,
    install::{InstallOptions, handle_install_command_async},
    uninstall::handle_uninstall_command,
    update::handle_update_command_async,
};

#[derive(Parser)]
#[command(name = "rnp")]
#[command(about = "Rust Node Package CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init {
        #[arg(short, long)]
        yes: bool,
    },
    #[command(visible_alias = "i")]
    Install {
        #[arg(long)]
        no_package_lock: bool,
        #[arg(short, long)]
        verbose: bool,
        #[arg(short, long)]
        quiet: bool,
        #[arg(required = true, num_args = 1..)]
        packages: Vec<String>,
    },
    Uninstall {
        #[arg(short, long)]
        quiet: bool,
        #[arg(required = true, num_args = 1..)]
        packages: Vec<String>,
    },
    Update {
        #[arg(long)]
        no_package_lock: bool,
        #[arg(short, long)]
        verbose: bool,
        #[arg(short, long)]
        quiet: bool,
        #[arg(num_args = 0..)]
        packages: Vec<String>,
    },
    // List,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { yes } => {
            handle_init(yes);
            Ok(())
        },
        Commands::Install {
            packages,
            no_package_lock,
            verbose,
            quiet,
        } => {
            let options = InstallOptions {
                no_package_lock,
                verbose,
                quiet,
            };

            for package in packages {
                handle_install_command_async(&package, options).await?;
            }

            Ok(())
        },
        Commands::Uninstall { quiet, packages } => {
            handle_uninstall_command(&packages, quiet)
        },
        Commands::Update {
            no_package_lock,
            verbose,
            quiet,
            packages,
        } => {
            let options = InstallOptions {
                no_package_lock,
                verbose,
                quiet,
            };
            handle_update_command_async(packages, options).await
        },
    }
}
