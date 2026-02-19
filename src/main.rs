mod color;
mod commands;
mod config;
mod preflight;
mod system;
mod template;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "slate")]
#[command(about = "Tool, not jailer.", version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize Slate (auto-detect hardware, create config, copy templates)
    Init,

    /// Verify LUKS encryption and system requirements
    Check {
        #[arg(long)]
        verbose: bool,
    },

    /// Render all templates and overwrite live configs
    Reload {
        #[arg(long)]
        dry_run: bool,
    },

    /// Wallpaper management
    Wall {
        #[command(subcommand)]
        action: WallAction,
    },

    /// Destructive system provisioning (THE FORGE)
    Forge {
        /// Target device (e.g., /dev/nvme0n1)
        device: String,
    },

    /// Internal stage runner (hidden)
    #[command(hide = true)]
    ChrootStage,
}

#[derive(Subcommand)]
enum WallAction {
    /// Set a wallpaper and optionally regenerate palette
    Set {
        /// Path to the wallpaper image
        path: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let home =
        home::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

    let config_path = home.join(".config/slate/slate.toml");

    match cli.command {
        Commands::Init => {
            commands::init()?;
        }
        Commands::Check { verbose } => {
            commands::check(verbose)?;
        }
        Commands::Reload { dry_run } => {
            // Check if config exists before trying to load
            if !config_path.exists() {
                eprintln!("\n[Slate] Config not found!");
                eprintln!("Run 'slate init' to set up Slate for the first time.\n");
                std::process::exit(1);
            }
            commands::reload(&config_path, dry_run)?;
        }
        Commands::Wall { action } => {
            if !config_path.exists() {
                eprintln!("[Slate] Config not found!");
                eprintln!("Run 'slate init' to set up Slate for the first time.");
                std::process::exit(1);
            }
            match action {
                WallAction::Set { path } => {
                    commands::wall_set(&config_path, &path)?;
                }
            }
        }
        Commands::Forge { device } => {
            commands::forge(&device)?;
        }
        Commands::ChrootStage => {
            commands::chroot_stage()?;
        }
    }

    Ok(())
}
