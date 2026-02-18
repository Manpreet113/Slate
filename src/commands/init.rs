use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::PathBuf;

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
    let uuid = detect_uuid()?;
    println!("    ✓ LUKS UUID: {}", uuid);

    // 3. Copy example config from repo and update UUID
    println!("  → Generating slate.toml...");
    let repo_dir = env::current_dir()?.canonicalize()?;
    let example_config_path = repo_dir.join("example.slate.toml");

    if !example_config_path.exists() {
        anyhow::bail!(
            "example.slate.toml not found in {}. Run slate init from the Slate repository directory.",
            repo_dir.display()
        );
    }

    // Read example config and update UUID
    let example_content = fs::read_to_string(&example_config_path)?;
    let updated_content = example_content.replace(
        "root_uuid = \"REPLACE_ME_RUN_SLATE_CHECK\"",
        &format!("root_uuid = \"{}\"", uuid),
    );

    fs::write(&config_path, updated_content)?;
    println!("    ✓ Saved to {}", config_path.display());

    // 4. Copy templates from repo
    println!("  → Copying templates...");
    let repo_templates = repo_dir.join("templates");

    if !repo_templates.exists() {
        anyhow::bail!(
            "templates/ not found in {}. Run slate init from the Slate repository directory.",
            repo_dir.display()
        );
    }

    copy_dir_recursive(&repo_templates, &templates_dir)?;
    println!("    ✓ All templates copied (24 app configs)");

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

fn detect_uuid() -> Result<String> {
    use crate::system;

    let root_device = system::get_root_device().context("Failed to detect root device")?;

    if !root_device.starts_with("/dev/mapper/") && !root_device.starts_with("/dev/dm-") {
        anyhow::bail!(
            "Slate requires LUKS encryption. Root device is {}, not a mapped device.",
            root_device
        );
    }

    let physical_device = system::trace_to_physical_partition(&root_device)
        .context("Failed to trace root device to physical partition")?;

    let uuid = system::get_uuid(&physical_device).context("Failed to get LUKS UUID")?;

    Ok(uuid)
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
