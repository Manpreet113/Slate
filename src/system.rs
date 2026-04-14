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

        // Skip pseudo and non-install targets.
        if name.starts_with("loop")
            || name.starts_with("ram")
            || name.starts_with("zram")
            || name.starts_with("dm-")
            || name.starts_with("md")
            || name.starts_with("sr")
            || name.starts_with("fd")
        {
            continue;
        }

        let device_path = block_dir.join(&name);
        if device_path.join("partition").exists() {
            continue;
        }

        // Get size
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

/// List all available keymaps in /usr/share/kbd/keymaps/
pub fn list_keymaps() -> Result<Vec<String>> {
    let mut keymaps = Vec::new();
    let base_path = Path::new("/usr/share/kbd/keymaps");
    if !base_path.exists() {
        return Ok(vec!["us".to_string()]); // Fallback
    }

    fn collect_maps(dir: &Path, keymaps: &mut Vec<String>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                collect_maps(&path, keymaps)?;
            } else if let Some(name) = path.file_name() {
                let s = name.to_string_lossy();
                if s.ends_with(".map.gz") || s.ends_with(".map") {
                    let map_name = s.trim_end_matches(".map.gz").trim_end_matches(".map");
                    keymaps.push(map_name.to_string());
                }
            }
        }
        Ok(())
    }

    collect_maps(base_path, &mut keymaps)?;
    keymaps.sort();
    keymaps.dedup();
    Ok(keymaps)
}

/// List all available timezones in /usr/share/zoneinfo/
pub fn list_timezones() -> Result<Vec<String>> {
    let mut zones = Vec::new();
    let base_path = Path::new("/usr/share/zoneinfo");
    if !base_path.exists() {
        return Ok(vec!["UTC".to_string()]); // Fallback
    }

    fn collect_zones(dir: &Path, base: &Path, zones: &mut Vec<String>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = path.file_name().unwrap_or_default().to_string_lossy();

            // Skip non-timezone files/folders
            if name.starts_with('.')
                || name == "posix"
                || name == "right"
                || name == "Etc"
                || name.ends_with(".tab")
            {
                continue;
            }

            if path.is_dir() {
                let _ = collect_zones(&path, base, zones);
            } else {
                if let Ok(relative) = path.strip_prefix(base) {
                    zones.push(relative.to_string_lossy().to_string());
                }
            }
        }
        Ok(())
    }

    let _ = collect_zones(base_path, base_path, &mut zones);
    zones.sort();
    Ok(zones)
}
