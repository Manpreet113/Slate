use anyhow::{bail, Context, Result};
use std::fs;
use std::process::Command;

pub fn check(verbose: bool) -> Result<()> {
    println!("[Slate] Checking system requirements...");
    
    // 1. Confirm Arch Linux
    let os_release = fs::read_to_string("/etc/os-release")
        .context("Failed to read /etc/os-release")?;
    
    if !os_release.contains("ID=arch") {
        bail!("Slate requires Arch Linux. This system is not Arch.");
    }
    
    if verbose {
        println!("✓ Running on Arch Linux");
    }
    
    // 2. Verify LUKS encryption
    let root_device = Command::new("findmnt")
        .args(["/", "-no", "SOURCE"])
        .output()
        .context("Failed to run findmnt")?;
    
    let root_device = String::from_utf8_lossy(&root_device.stdout).trim().to_string();
    
    if !root_device.starts_with("/dev/mapper/") {
        bail!(
            "Hardware mismatch. Root device is {}.\n\
            Slate strictly requires a LUKS encrypted root partition.\n\
            Did you forget to enable encryption during archinstall?",
            root_device
        );
    }
    
    if verbose {
        println!("✓ LUKS encryption detected: {}", root_device);
    }
    
    // 3. Verify we can trace to physical device
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
    
    if verbose {
        println!("✓ Physical device: {}", physical_device);
    }
    
    // 4. Extract PARTUUID (requires sudo)
    let partuuid_output = Command::new("sudo")
        .args(["blkid", "-s", "PARTUUID", "-o", "value", &physical_device])
        .output()
        .context("Failed to run blkid (you may need to enter your password)")?;
    
    let partuuid = String::from_utf8_lossy(&partuuid_output.stdout).trim().to_string();
    
    if partuuid.is_empty() {
        bail!("Could not extract PARTUUID from {}. Try running: sudo blkid -s PARTUUID -o value {}", 
              physical_device, physical_device);
    }
    
    println!("✓ Root PARTUUID: {}", partuuid);
    
    // 5. Check for required packages
    let required_packages = [
        "hyprland",
        "waybar",
        "ghostty",
        "mako",
        "rofi",
        "plymouth",
    ];
    
    if verbose {
        println!("\n[Slate] Checking required packages...");
        for pkg in &required_packages {
            let status = Command::new("pacman")
                .args(["-Qi", pkg])
                .output();
            
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
