use clap::{Parser, Subcommand};

mod cache;
mod commands;
use commands::{
    audit::handle_audit_command_async,
    init::handle_init,
    install::{InstallOptions, handle_ci_command_async, handle_install_command_async},
    run::handle_run_command,
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
        #[arg(long)]
        ignore_scripts: bool,
        #[arg(short = 'w', long)]
        workspace: Option<String>,
        #[arg(long, default_value = "safe", value_parser = ["none", "safe", "aggressive"])]
        hoist: String,
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
        #[arg(long)]
        ignore_scripts: bool,
        #[arg(short = 'w', long)]
        workspace: Option<String>,
        #[arg(long, default_value = "safe", value_parser = ["none", "safe", "aggressive"])]
        hoist: String,
        #[arg(short, long)]
        verbose: bool,
        #[arg(short, long)]
        quiet: bool,
        #[arg(num_args = 0..)]
        packages: Vec<String>,
    },
    Ci {
        #[arg(long)]
        ignore_scripts: bool,
        #[arg(short = 'w', long)]
        workspace: Option<String>,
        #[arg(long, default_value = "safe", value_parser = ["none", "safe", "aggressive"])]
        hoist: String,
        #[arg(short, long)]
        verbose: bool,
        #[arg(short, long)]
        quiet: bool,
    },
    Run {
        script: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    Audit,
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
            ignore_scripts,
            workspace,
            hoist,
            verbose,
            quiet,
        } => {
            let options = InstallOptions {
                no_package_lock,
                verbose,
                quiet,
                ignore_scripts,
                workspace,
                hoist_strategy: hoist,
            };

            for package in packages {
                handle_install_command_async(&package, options.clone()).await?;
            }

            Ok(())
        },
        Commands::Uninstall { quiet, packages } => {
            handle_uninstall_command(&packages, quiet)
        },
        Commands::Update {
            no_package_lock,
            ignore_scripts,
            workspace,
            hoist,
            verbose,
            quiet,
            packages,
        } => {
            let options = InstallOptions {
                no_package_lock,
                verbose,
                quiet,
                ignore_scripts,
                workspace,
                hoist_strategy: hoist,
            };
            handle_update_command_async(packages, options).await
        },
        Commands::Ci {
            ignore_scripts,
            workspace,
            hoist,
            verbose,
            quiet,
        } => {
            let options = InstallOptions {
                no_package_lock: false,
                verbose,
                quiet,
                ignore_scripts,
                workspace,
                hoist_strategy: hoist,
            };
            handle_ci_command_async(options).await
        },
        Commands::Run { script, args } => {
            handle_run_command(&script, &args)
        },
        Commands::Audit => {
            handle_audit_command_async().await
        },
    }
}
