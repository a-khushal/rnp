use clap::{Parser, Subcommand};

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
        Commands::Install { package } => {
            handle_install_command_async(&package).await
        },
    }
}
