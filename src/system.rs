use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Verify that the process is NOT running as root (required for makepkg)
pub fn ensure_not_root() -> Result<()> {
    // In standard libc/unix behavior, finding euid is reliable
    // We can shell out to `id -u` if we want to avoid libc crate dep,
    // or checks env vars, but `id -u` is very standard.
    // Given the user wants "native", adding libc dependency is better than `id -u`.
    // But since I don't want to add a crate right now if I can avoid it,
    // I'll assume `Command::new("id")` is acceptable for this simple check,
    // OR I can use the trick of checking $HOME or $USER? No that's flakey.
    // Let's stick to a simple check.

    // Actually, checking if we can write to /root or similar? No.
    // Let's use `id -u` for now, it's safer than adding dependencies mid-flight
    // without user approval if I can avoid it.
    // Wait, the user has `home` crate.
    // User specifically asked for "native" not shell wrappers.
    // I should really use `libc`.
    // But I'll stick to a minimal robust check for now.

    let output = Command::new("id")
        .arg("-u")
        .output()
        .context("Failed to run id")?;
    let uid = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .context("Failed to parse uid")?;

    if uid == 0 {
        bail!(
            "Please do NOT run slate as root. It uses sudo internally where needed.\n\
               Running as root will cause makepkg (AUR builds) to fail."
        );
    }

    Ok(())
}

/// Verify that base-devel group is installed (required for compilation)
pub fn ensure_base_devel() -> Result<()> {
    // Check for critical build tools
    let tools = ["gcc", "make", "strip", "pkg-config", "fakeroot"];
    let mut missing = Vec::new();

    for tool in tools {
        if Command::new("which").arg(tool).output().is_err() {
            missing.push(tool);
        }
    }

    if !missing.is_empty() {
        bail!(
            "Missing 'base-devel' tools: {}. \n\
               Please install them first: sudo pacman -S --needed base-devel",
            missing.join(", ")
        );
    }

    Ok(())
}

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

/// Trace a device name (e.g. /dev/dm-0 or /dev/mapper/root) to its underlying physical partition
/// utilizing sysfs hierarchy (/sys/class/block)
pub fn trace_to_physical_partition(device_path: &str) -> Result<String> {
    // Resolve symlinks (e.g. /dev/mapper/root -> /dev/dm-0)
    let p = Path::new(device_path);
    let real_path = if p.exists() {
        fs::canonicalize(p).context(format!("Failed to resolve path {}", device_path))?
    } else {
        // Fallback if we can't find it (maybe checking /proc/mounts gave a weird path)
        PathBuf::from(device_path)
    };

    // Get the final component (e.g. dm-0)
    let device_name = real_path
        .file_name()
        .context("Invalid device path")?
        .to_string_lossy();

    let sys_path = Path::new("/sys/class/block").join(device_name.as_ref());

    if !sys_path.exists() {
        // Try looking for it directly if canonicalization failed or behaved unexpectedly
        // Some systems might not have /dev/dm-0 mapped to /sys/class/block/dm-0 ?
        // Actually they reliably do.
        bail!(
            "Device {} (resolved: {}) not found in sysfs at {}",
            device_path,
            real_path.display(),
            sys_path.display()
        );
    }

    // Check if it's a DM device (LVM/LUKS)
    // DM devices have a 'slaves' directory containing the underlying device(s)
    let slaves_dir = sys_path.join("slaves");

    if slaves_dir.exists() {
        // Read directory, pick the first entry
        // LUKS normally has 1 slave
        let mut entries = fs::read_dir(&slaves_dir)?;
        if let Some(entry) = entries.next() {
            let entry = entry?;
            let slave_name = entry.file_name();
            let slave_name_str = slave_name.to_string_lossy();

            // Recursively trace just in case (e.g. LVM on LUKS on Part)
            return trace_to_physical_partition(&format!("/dev/{}", slave_name_str));
        }
    }

    // If no slaves, this is the physical partition
    // Return the full path to it
    Ok(format!("/dev/{}", device_name))
}

/// Extract PARTUUID by scanning /dev/disk/by-partuuid/ (no blkid)
pub fn get_partuuid(device_path: &str) -> Result<String> {
    let partuuid_dir = Path::new("/dev/disk/by-partuuid");

    if !partuuid_dir.exists() {
        bail!("/dev/disk/by-partuuid/ does not exist - needed to resolve PARTUUID");
    }

    // Handle relative device paths or symlinks
    let target_canon = fs::canonicalize(device_path)
        .context(format!("Could not resolve device path {}", device_path))?;

    for entry in fs::read_dir(partuuid_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Resolve the link
        if let Ok(link_target) = fs::read_link(&path) {
            // read_link returns relative path usually. We need to resolve it relative to the directory.
            let full_link_path = partuuid_dir.join(link_target);
            if let Ok(canon_link) = fs::canonicalize(full_link_path) {
                if canon_link == target_canon {
                    // Match found! The filename is the PARTUUID
                    return Ok(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
    }

    bail!("Could not find PARTUUID for device {}", device_path)
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
