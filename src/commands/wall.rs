use crate::config::SlateConfig;
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

pub fn wall_set(config_path: &Path, image_path: &str) -> Result<()> {
    // Expand ~ to home directory
    let expanded = shellexpand::tilde(image_path);
    let source = Path::new(expanded.as_ref());

    if !source.exists() {
        bail!("Wallpaper not found: {}", source.display());
    }

    // Ensure it's an image
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !["png", "jpg", "jpeg", "webp", "bmp"].contains(&ext.as_str()) {
        bail!(
            "Unsupported image format: .{}\nSupported: png, jpg, jpeg, webp, bmp",
            ext
        );
    }

    let mut config = SlateConfig::load(config_path)?;

    // Copy to standard location
    let home =
        home::home_dir().ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
    let wall_dir = home.join("Pictures/Wallpapers");
    std::fs::create_dir_all(&wall_dir)?;

    let dest = wall_dir.join(source.file_name().unwrap());
    if source != dest {
        std::fs::copy(source, &dest).context("Failed to copy wallpaper")?;
        println!("[Slate] Copied wallpaper to {}", dest.display());
    } else {
        println!("[Slate] Wallpaper is already in the target directory.");
    }

    // Update config
    let wall_path = format!(
        "~/Pictures/Wallpapers/{}",
        source.file_name().unwrap().to_str().unwrap()
    );
    config.hardware.wallpaper = wall_path.clone();

    // If matugen mode, regenerate palette
    if config.palette.mode == "matugen" {
        println!("[Slate] Generating palette from wallpaper (matugen)...");
        match generate_palette_from_wallpaper(&dest.to_string_lossy()) {
            Ok(palette_colors) => {
                config.palette.bg_void_transparent = format!("{}99", &palette_colors.bg_void[..7]);
                config.palette.bg_void = palette_colors.bg_void;
                config.palette.bg_surface = palette_colors.bg_surface;
                config.palette.bg_overlay = palette_colors.bg_overlay;
                config.palette.foreground = palette_colors.foreground;
                config.palette.foreground_dim = palette_colors.foreground_dim;
                config.palette.accent = palette_colors.accent;
                config.palette.accent_bright = palette_colors.accent_bright;
                println!("  ✓ Palette generated from wallpaper");
            }
            Err(e) => {
                eprintln!("  ⚠ matugen failed: {}. Keeping current palette.", e);
            }
        }
    }

    config.save(config_path)?;
    println!("  ✓ Updated slate.toml");

    // Trigger reload to apply everywhere
    println!("\n[Slate] Applying changes...");
    crate::commands::reload(config_path, false)?;

    Ok(())
}

struct MatugenPalette {
    bg_void: String,
    bg_surface: String,
    bg_overlay: String,
    foreground: String,
    foreground_dim: String,
    accent: String,
    accent_bright: String,
}

fn generate_palette_from_wallpaper(image_path: &str) -> Result<MatugenPalette> {
    let output = Command::new("matugen")
        .args([
            "image",
            image_path,
            "--json",
            "hex",
            "-t",
            "scheme-tonal-spot",
        ])
        .output()
        .context("Failed to run matugen. Is it installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("matugen exited with error: {}", stderr);
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("Failed to parse matugen JSON output")?;

    // matugen outputs { "colors": { "dark": { "surface": "#xxx", ... } } }
    let dark = json
        .get("colors")
        .and_then(|c| c.get("dark"))
        .ok_or_else(|| anyhow::anyhow!("matugen output missing colors.dark"))?;

    let get_color = |key: &str| -> String {
        dark.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("#000000")
            .to_string()
    };

    Ok(MatugenPalette {
        bg_void: get_color("surface"),
        bg_surface: get_color("surface_container"),
        bg_overlay: get_color("surface_container_high"),
        foreground: get_color("on_surface"),
        foreground_dim: get_color("on_surface_variant"),
        accent: get_color("primary"),
        accent_bright: get_color("primary_container"),
    })
}
