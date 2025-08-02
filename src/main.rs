use clap::{Parser, Subcommand};

mod commands;
use commands::{init::handle_init, install::handle_install_command};

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
    Install { package: String },
    // List,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { yes } => handle_init(yes),
        Commands::Install { package } => handle_install_command(&package),
        _ => {}
        // Commands::List => handle_list(),
    }
}