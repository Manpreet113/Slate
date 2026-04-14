mod commands;
mod installer;
mod system;
mod tui;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "slate")]
#[command(about = "Arch Linux installer for the Slate shell", version = "0.2.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the interactive TUI installer
    Install,

    /// Verify system requirements before installation
    Check {
        #[arg(long)]
        verbose: bool,
    },

    /// Internal stage runner (hidden)
    #[command(hide = true)]
    ChrootStage,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Install => {
            commands::forge()?;
        }
        Commands::Check { verbose } => {
            commands::check(verbose)?;
        }
        Commands::ChrootStage => {
            commands::chroot_stage()?;
        }
    }

    Ok(())
}
