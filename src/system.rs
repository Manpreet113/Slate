use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

/// Find the root device using /proc/mounts (no findmnt)
pub fn get_root_device() -> Result<String> {
    let mounts = fs::read_to_string("/proc/mounts").context("Failed to read /proc/mounts")?;

    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let device = parts[0];
            let mount_point = parts[1];

            if mount_point == "/" {
                return Ok(device.to_string());
            }
        }
    }

    bail!("Could not identify root filesystem in /proc/mounts")
}



/// Extract filesystem/LUKS UUID by scanning /dev/disk/by-uuid/
pub fn get_uuid(device_path: &str) -> Result<String> {
    let uuid_dir = Path::new("/dev/disk/by-uuid");

    if !uuid_dir.exists() {
        bail!("/dev/disk/by-uuid/ does not exist - needed to resolve UUID");
    }

    // Handle relative device paths or symlinks
    let target_canon = fs::canonicalize(device_path)
        .context(format!("Could not resolve device path {}", device_path))?;

    for entry in fs::read_dir(uuid_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Resolve the link
        if let Ok(link_target) = fs::read_link(&path) {
            let full_link_path = uuid_dir.join(link_target);
            if let Ok(canon_link) = fs::canonicalize(full_link_path) {
                if canon_link == target_canon {
                    return Ok(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
    }

    bail!("Could not find UUID for device {}", device_path)
}

#[derive(Debug, Clone)]
pub struct BlockDevice {
    pub path: String,
    pub size: String,
    pub model: String,
}

/// List all available physical block devices (disks, not partitions)
pub fn list_block_devices() -> Result<Vec<BlockDevice>> {
    let mut devices = Vec::new();
    let block_dir = Path::new("/sys/class/block");

    if !block_dir.exists() {
        bail!("Could not access /sys/class/block");
    }

    for entry in fs::read_dir(block_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();

        // Skip partitions (e.g. sda1, nvme0n1p1) and loop devices
        if name.starts_with("loop") || name.contains('p') || (name.starts_with("sd") && name.len() > 3) {
            // This is a bit naive but works for common naming schemes.
            // A better way is checking /sys/class/block/NAME/partition file existence.
            continue;
        }

        let device_path = block_dir.join(&name);
        
        // Ensure it's not a partition
        if device_path.join("partition").exists() {
            continue;
        }

        // Get size (in 512-byte blocks)
        let size_str = fs::read_to_string(device_path.join("size")).unwrap_or_default();
        let size_bytes = size_str.trim().parse::<u64>().unwrap_or(0) * 512;
        let size_gb = size_bytes as f64 / 1024.0 / 1024.0 / 1024.0;

        // Get model
        let model = fs::read_to_string(device_path.join("device/model"))
            .unwrap_or_else(|_| "Unknown".to_string())
            .trim()
            .to_string();

        devices.push(BlockDevice {
            path: format!("/dev/{}", name),
            size: format!("{:.1} GB", size_gb),
            model,
        });
    }

    Ok(devices)
}
