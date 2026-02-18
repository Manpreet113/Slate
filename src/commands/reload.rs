use crate::config::{ReloadSignal, SlateConfig};
use crate::template::TemplateEngine;
use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub fn reload(config_path: &PathBuf, dry_run: bool) -> Result<()> {
    let config = SlateConfig::load(config_path)?;
    
    let home = home::home_dir().context("Could not determine home directory")?;
    let templates_dir = config_path
        .parent()
        .context("Could not determine config directory")?
        .join("templates");
    
    let config_root = home.join(".config");
    
    println!("[Slate] Initializing template engine...");
    let engine = TemplateEngine::new(
        templates_dir
            .to_str()
            .context("Invalid templates directory path")?
    )?;
    
    // Step 1: Render all templates to memory
    println!("[Slate] Rendering templates...");
    let mut renders = Vec::new();
    
    for app in &config.apps {
        if !app.enabled {
            continue;
        }
        
        println!("  → {}", app.template_path);
        let content = engine.render(&app.template_path, &config)
            .with_context(|| format!("Failed to render template: {}", app.template_path))?;
        
        renders.push((app.clone(), content));
    }
    
    if dry_run {
        println!("\n[DRY RUN] Would write the following configs:");
        for (app, content) in &renders {
            println!("\n━━━ ~/.config/{} ━━━", app.config_path);
            println!("{}", content);
        }
        return Ok(());
    }
    
    // Step 2: Write all to .tmp files
    println!("\n[Slate] Writing temporary files...");
    let mut temp_files = Vec::new();
    
    for (app, content) in &renders {
        let target = config_root.join(&app.config_path);
        let temp = target.with_extension("tmp");
        
        if let Some(parent) = temp.parent() {
            fs::create_dir_all(parent)?;
        }
        
        fs::write(&temp, content)?;
        temp_files.push((temp.clone(), target.clone()));
    }
    
    // Step 3: Atomic rename all .tmp → final
    println!("[Slate] Committing configs...");
    for (temp, target) in temp_files {
        fs::rename(temp, &target)?;
        println!("  ✓ {}", target.strip_prefix(&config_root)?.display());
    }
    
    // Step 4: Fire all reload signals
    println!("\n[Slate] Propagating reload signals...");
    for (app, _) in renders {
        send_reload_signal(&app.reload_signal, &app.name)?;
    }
    
    println!("\n[Slate] Reload complete.");
    Ok(())
}

fn send_reload_signal(signal: &ReloadSignal, app_name: &str) -> Result<()> {
    match signal {
        ReloadSignal::Hyprctl => {
            println!("  → hyprctl reload");
            Command::new("hyprctl")
                .arg("reload")
                .output()
                .ok(); // Don't fail if hyprland isn't running
        },
        ReloadSignal::Signal { signal } => {
            println!("  → pkill -SIGUSR2 {}", signal);
            Command::new("pkill")
                .arg("-SIGUSR2")
                .arg(signal)
                .output()
                .ok(); // Don't fail if app isn't running
        },
        ReloadSignal::Makoctl => {
            println!("  → makoctl reload");
            Command::new("makoctl")
                .arg("reload")
                .output()
                .ok(); // Don't fail if mako isn't running
        },
        ReloadSignal::None => {
            println!("  → {} (no reload signal)", app_name);
        },
    }
    Ok(())
}
