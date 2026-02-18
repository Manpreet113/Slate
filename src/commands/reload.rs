use crate::config::{ReloadSignal, SlateConfig};
use crate::template::TemplateEngine;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn reload(config_path: &Path, dry_run: bool) -> Result<()> {
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
            .context("Invalid templates directory path")?,
    )?;

    // Step 1: Render all templates to memory
    println!("[Slate] Rendering templates...");
    let mut renders = Vec::new();

    for app in &config.apps {
        if !app.enabled {
            continue;
        }

        println!("  → {}", app.template_path);
        let content = engine
            .render(&app.template_path, &config)
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
    // Step 4: Fire all reload signals (Deduplicated)
    println!("\n[Slate] Propagating reload signals...");

    let mut executed_signals = std::collections::HashSet::new();

    for (app, _) in renders {
        match &app.reload_signal {
            ReloadSignal::None => {
                println!("  → {} (no reload signal)", app.name);
            }
            signal => {
                // Generate a unique signature for the command
                let sig_key = match signal {
                    ReloadSignal::Hyprctl => "hyprctl".to_string(),
                    ReloadSignal::Hyprpaper => "hyprpaper".to_string(),
                    ReloadSignal::Signal { signal: s } => format!("signal:{}", s),
                    ReloadSignal::Makoctl => "makoctl".to_string(),
                    ReloadSignal::None => unreachable!(),
                };

                // insert() returns true if the value was NOT already present
                if executed_signals.insert(sig_key) {
                    send_reload_signal(signal, &config)?;
                }
            }
        }
    }

    println!("\n[Slate] Reload complete.");
    Ok(())
}

fn send_reload_signal(signal: &ReloadSignal, _config: &SlateConfig) -> Result<()> {
    match signal {
        ReloadSignal::Hyprctl => {
            println!("  → hyprctl reload");
            Command::new("hyprctl").arg("reload").output().ok();
        }
        ReloadSignal::Hyprpaper => {
            println!("  → restarting hyprpaper");
            Command::new("pkill").arg("hyprpaper").output().ok();

            // Wait for clean exit
            std::thread::sleep(std::time::Duration::from_millis(500));

            Command::new("hyprpaper").spawn().ok();
        }
        ReloadSignal::Signal { signal } => {
            println!("  → pkill -SIGUSR2 {}", signal);
            Command::new("pkill")
                .arg("-SIGUSR2")
                .arg(signal)
                .output()
                .ok();
        }
        ReloadSignal::Makoctl => {
            println!("  → makoctl reload");
            Command::new("makoctl").arg("reload").output().ok();
        }
        ReloadSignal::None => {}
    }
    Ok(())
}
