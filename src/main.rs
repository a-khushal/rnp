use clap::{Parser, Subcommand};

mod cache;
mod commands;
use commands::{init::handle_init, install::handle_install_command_async};

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
    Install {
        #[arg(long)]
        no_package_lock: bool,
        package: String,
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
            package,
            no_package_lock,
        } => {
            handle_install_command_async(&package, no_package_lock).await
        },
    }
}
