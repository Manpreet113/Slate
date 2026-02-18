use anyhow::{bail, Context, Result};
use std::fs;
use std::process::Command;

pub fn check(verbose: bool) -> Result<()> {
    println!("[Slate] Checking system requirements...");

    // 1. Confirm Arch Linux
    let os_release =
        fs::read_to_string("/etc/os-release").context("Failed to read /etc/os-release")?;

    if !os_release.contains("ID=arch") {
        bail!("Slate requires Arch Linux. This system is not Arch.");
    }

    if verbose {
        println!("✓ Running on Arch Linux");
    }

    // 2. Verify LUKS encryption
    let root_device = crate::system::get_root_device()?;

    if !root_device.starts_with("/dev/mapper/") && !root_device.starts_with("/dev/dm-") {
        bail!(
            "Hardware mismatch. Root device is {}.\n\
            Slate strictly requires a LUKS encrypted root partition.",
            root_device
        );
    }

    if verbose {
        println!("✓ LUKS encryption detected: {}", root_device);
    }

    // 3. Verify we can trace to physical device
    let physical_device = crate::system::trace_to_physical_partition(&root_device)
        .context("Could not resolve physical parent device")?;

    if verbose {
        println!("✓ Physical device: {}", physical_device);
    }

    // 4. Extract PARTUUID
    let partuuid =
        crate::system::get_partuuid(&physical_device).context("Could not extract PARTUUID")?;

    println!("✓ Root PARTUUID: {}", partuuid);

    // 5. Check for required packages
    let required_packages = ["hyprland", "waybar", "ghostty", "mako", "rofi", "plymouth"];

    if verbose {
        println!("\n[Slate] Checking required packages...");
        for pkg in &required_packages {
            let status = Command::new("pacman").args(["-Qi", pkg]).output();

            match status {
                Ok(output) if output.status.success() => {
                    println!("  ✓ {}", pkg);
                }
                _ => {
                    println!("  ✗ {} (not installed)", pkg);
                }
            }
        }
    }

    println!("\n[Slate] System check complete. Ready to operate.");
    Ok(())
}
