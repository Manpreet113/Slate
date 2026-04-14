use anyhow::{bail, Result};
use nix::unistd::Uid;
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn run_checks(device: &str) -> Result<Vec<String>> {
    let mut logs = Vec::new();

    // 1. Check Root
    if !Uid::effective().is_root() {
        bail!("ROOT privileges required.");
    }
    logs.push("✓ Root access verified".to_string());

    // 2. Check UEFI
    if !Path::new("/sys/firmware/efi").exists() {
        bail!("Slate requires UEFI mode.");
    }
    logs.push("✓ UEFI mode verified".to_string());

    // 3. Check Device
    if !Path::new(device).exists() {
        bail!("Target device {} not found.", device);
    }
    logs.push(format!("✓ Target device verified: {}", device));

    // 4. Check Mounts
    check_mounts(device)?;
    logs.push("✓ Mount check passed".to_string());

    // 5. Check Tools
    check_tools()?;
    logs.push("✓ Required tools verified".to_string());

    // 6. Check Network
    check_network()?;
    logs.push("✓ Network connectivity verified".to_string());

    Ok(logs)
}

fn check_mounts(device: &str) -> Result<()> {
    let mounts = fs::read_to_string("/proc/mounts")?;
    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let source = parts[0];
        let is_target = source == device
            || source.starts_with(&format!("{}p", device))
            || (source.starts_with(device)
                && source
                    .chars()
                    .nth(device.len())
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false));

        if is_target {
            bail!("Device {} is currently mounted!", device);
        }
    }
    Ok(())
}

fn check_tools() -> Result<()> {
    let tools = [
        "sgdisk",
        "mkfs.btrfs",
        "mkfs.vfat",
        "btrfs",
        "arch-chroot",
        "pacstrap",
        "mkinitcpio",
        "bootctl",
        "curl",
        "reflector",
    ];
    let mut missing = Vec::new();
    for tool in tools {
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {}", tool))
            .status();
        if !status.map(|s| s.success()).unwrap_or(false) {
            missing.push(tool);
        }
    }
    if !missing.is_empty() {
        bail!("Missing tools: {}", missing.join(", "));
    }
    Ok(())
}

fn check_network() -> Result<()> {
    let status = Command::new("curl")
        .args(["-I", "https://archlinux.org"])
        .status();
    if !status.map(|s| s.success()).unwrap_or(false) {
        bail!("Network unreachable.");
    }
    Ok(())
}
