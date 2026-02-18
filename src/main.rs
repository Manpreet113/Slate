mod color;
mod config;
mod template;
mod commands;
mod system;

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
    /// Full system installation (packages, bootloader, configs) - replaces install.sh
    Install,
    
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
    
    /// Update slate.toml and trigger reload
    Set {
        key: String,
        value: String,
        #[arg(long)]
        dry_run: bool,
    },
    
    /// Wallpaper management
    Wall {
        #[command(subcommand)]
        action: WallAction,
    },
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
    
    let home = home::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    
    let config_path = home.join(".config/slate/slate.toml");
    
    match cli.command {
        Commands::Install => {
            commands::install()?;
        },
        Commands::Init => {
            commands::init()?;
        },
        Commands::Check { verbose } => {
            commands::check(verbose)?;
        },
        Commands::Reload { dry_run } => {
            // Check if config exists before trying to load
            if !config_path.exists() {
                eprintln!("\n[Slate] Config not found!");
                eprintln!("Run 'slate init' to set up Slate for the first time.\n");
                std::process::exit(1);
            }
            commands::reload(&config_path, dry_run)?;
        },
        Commands::Set { key, value, dry_run } => {
            if !config_path.exists() {
                eprintln!("[Slate] Config not found!");
                eprintln!("Run 'slate init' to set up Slate for the first time.");
                std::process::exit(1);
            }
            commands::set(&config_path, &key, &value, dry_run)?;
        },
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
        },
    }
    
    Ok(())
}
