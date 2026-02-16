use crate::config::SlateConfig;
use anyhow::{bail, Context, Result};
use std::path::PathBuf;

pub fn set(config_path: &PathBuf, key: &str, value: &str, dry_run: bool) -> Result<()> {
    let mut config = SlateConfig::load(config_path)?;
    
    // Parse dot notation keys
    let parts: Vec<&str> = key.split('.').collect();
    
    match parts.as_slice() {
        ["palette", "bg_void"] => {
            config.palette.bg_void = value.to_string();
        },
        ["palette", "foreground"] => {
            config.palette.foreground = value.to_string();
        },
        ["palette", "accent"] => {
            config.palette.accent = value.to_string();
        },
        ["hardware", "monitor_scale"] => {
            let scale: f32 = value.parse()
                .context("monitor_scale must be a number")?;
            config.hardware.monitor_scale = scale;
        },
        ["hardware", "font_family"] => {
            config.hardware.font_family = value.to_string();
        },
        ["hardware", "root_partuuid"] => {
            config.hardware.root_partuuid = value.to_string();
        },
        _ => {
            bail!("Unknown configuration key: {}\nValid keys:\n  palette.bg_void\n  palette.foreground\n  palette.accent\n  hardware.monitor_scale\n  hardware.font_family\n  hardware.root_partuuid", key);
        },
    }
    
    if dry_run {
        println!("[DRY RUN] Would update {} = {}", key, value);
        println!("\nNew configuration:");
        println!("{}", toml::to_string_pretty(&config)?);
        return Ok(());
    }
    
    println!("[Slate] Updating {} = {}", key, value);
    config.save(config_path)?;
    println!("  ✓ Saved to {}", config_path.display());
    
    // Auto-trigger reload
    println!("\n[Slate] Triggering reload...");
    crate::commands::reload(config_path, false)?;
    
    Ok(())
}
