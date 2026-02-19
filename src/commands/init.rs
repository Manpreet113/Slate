use anyhow::{Context, Result};
use std::fs;

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

    // 3. Write embedded example config and update UUID
    println!("  → Generating slate.toml...");
    // Embed example config at compile time
    let example_content = include_str!("../../example.slate.toml");

    let updated_content = example_content.replace(
        "root_uuid = \"REPLACE_ME_RUN_SLATE_CHECK\"",
        &format!("root_uuid = \"{}\"", uuid),
    );

    fs::write(&config_path, updated_content)?;
    println!("    ✓ Saved to {}", config_path.display());

    // 4. Write embedded templates
    println!("  → Deploying embedded templates...");

    // Use the embedded templates exposed in template.rs
    for (name, content) in crate::template::EMBEDDED_TEMPLATES {
        let dest_path = templates_dir.join(name);

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&dest_path, content)?;
    }

    println!(
        "    ✓ All templates deployed ({} files)",
        crate::template::EMBEDDED_TEMPLATES.len()
    );

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

    // Use get_root_device but handle Chroot environment?
    // slate init is usually run by user on a live system.
    // If running in chroot, get_root_device might return /dev/mapper/root correctly.

    let root_device = system::get_root_device().context("Failed to detect root device")?;

    if !root_device.starts_with("/dev/mapper/") && !root_device.starts_with("/dev/dm-") {
        // Warning instead of bail?
        // No, Slate is for LUKS setups.
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
