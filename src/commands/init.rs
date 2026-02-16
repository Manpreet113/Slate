use crate::config::{App, Hardware, Palette, ReloadSignal, SlateConfig};
use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub fn init() -> Result<()> {
    println!("[Slate] Initializing configuration...");
    
    let home = home::home_dir().context("Could not determine home directory")?;
    let config_dir = home.join(".config/slate");
    let templates_dir = config_dir.join("templates");
    let config_path = config_dir.join("slate.toml");
    
    // 1. Create directories
    println!("  → Creating ~/.config/slate/");
    fs::create_dir_all(&templates_dir)?;
    
    // 2. Auto-detect hardware
    println!("  → Detecting hardware configuration...");
    let partuuid = detect_partuuid()?;
    println!("    ✓ PARTUUID: {}", partuuid);
    
    // 3. Generate default config
    println!("  → Generating slate.toml...");
    let config = SlateConfig {
        palette: Palette {
            bg_void: "#0b0c10".to_string(),
            foreground: "#aeb3c2".to_string(),
            accent: "#ffffff".to_string(),
        },
        hardware: Hardware {
            monitor_scale: 1.0,
            root_partuuid: partuuid,
            font_family: "Iosevka Nerd Font".to_string(),
        },
        apps: vec![
            App {
                name: "waybar-style".to_string(),
                enabled: true,
                template_path: "waybar/style.css".to_string(),
                config_path: "waybar/style.css".to_string(),
                reload_signal: ReloadSignal::Signal { signal: "waybar".to_string() },
            },
            App {
                name: "waybar-config".to_string(),
                enabled: true,
                template_path: "waybar/config".to_string(),
                config_path: "waybar/config".to_string(),
                reload_signal: ReloadSignal::Signal { signal: "waybar".to_string() },
            },
            App {
                name: "ghostty".to_string(),
                enabled: true,
                template_path: "ghostty/config".to_string(),
                config_path: "ghostty/config".to_string(),
                reload_signal: ReloadSignal::None,
            },
        ],
    };
    
    config.save(&config_path)?;
    println!("    ✓ Saved to {}", config_path.display());
    
    // 4. Copy templates from repo
    println!("  → Copying templates...");
    let repo_dir = env::current_dir()?.canonicalize()?;
    let repo_templates = repo_dir.join("templates");
    
    if !repo_templates.exists() {
        println!("    ⚠ Warning: templates/ not found in current directory");
        println!("    You'll need to manually copy templates to {}", templates_dir.display());
    } else {
        copy_dir_recursive(&repo_templates, &templates_dir)?;
        println!("    ✓ Templates copied");
    }
    
    // 5. Run initial reload
    println!("\n[Slate] Running initial config generation...");
    crate::commands::reload(&config_path, false)?;
    
    println!("\n[Slate] Initialization complete!");
    println!("  Config: {}", config_path.display());
    println!("  Templates: {}", templates_dir.display());
    println!("\nYou can now:");
    println!("  • Run 'slate reload' to regenerate configs");
    println!("  • Run 'slate set palette.accent \"#5f87af\"' to change colors");
    println!("  • Edit templates in {}", templates_dir.display());
    
    Ok(())
}

fn detect_partuuid() -> Result<String> {
    // Same logic as check.rs
    let root_device = Command::new("findmnt")
        .args(["/", "-no", "SOURCE"])
        .output()
        .context("Failed to run findmnt")?;
    
    let root_device = String::from_utf8_lossy(&root_device.stdout).trim().to_string();
    
    if !root_device.starts_with("/dev/mapper/") {
        anyhow::bail!(
            "Slate requires LUKS encryption. Root device is {}, not a /dev/mapper/* device",
            root_device
        );
    }
    
    let dm_name = root_device.trim_start_matches("/dev/mapper/");
    
    let lsblk_output = Command::new("lsblk")
        .args(["-nro", "NAME,PKNAME"])
        .output()
        .context("Failed to run lsblk")?;
    
    let lsblk_output = String::from_utf8_lossy(&lsblk_output.stdout);
    
    let parent_name = lsblk_output
        .lines()
        .find_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[0] == dm_name {
                Some(parts[1].to_string())
            } else {
                None
            }
        })
        .context("Could not resolve physical parent device")?;
    
    let physical_device = format!("/dev/{}", parent_name);
    
    let partuuid_output = Command::new("sudo")
        .args(["blkid", "-s", "PARTUUID", "-o", "value", &physical_device])
        .output()
        .context("Failed to run blkid")?;
    
    let partuuid = String::from_utf8_lossy(&partuuid_output.stdout).trim().to_string();
    
    if partuuid.is_empty() {
        anyhow::bail!("Could not extract PARTUUID from {}", physical_device);
    }
    
    Ok(partuuid)
}

fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    fs::create_dir_all(dst)?;
    
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(&file_name);
        
        if path.is_dir() {
            copy_dir_recursive(&path, &dst_path)?;
        } else {
            fs::copy(&path, &dst_path)?;
        }
    }
    
    Ok(())
}
