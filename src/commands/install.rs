use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub fn install() -> Result<()> {
    crate::system::ensure_not_root()?;
    crate::system::ensure_base_devel()?;

    println!("[Slate] Full system installation starting...");
    println!("[Slate] This will install packages, configure bootloader, and set up system files.\n");
    
    
    // 1. Ensure ax is up-to-date (always download latest)
    println!("[Slate] Installing/updating ax package manager...");
    
    let temp_dir = std::env::temp_dir().join("ax-install");
    if temp_dir.exists() { fs::remove_dir_all(&temp_dir)?; }
    fs::create_dir_all(&temp_dir)?;
    
    let ax_binary = temp_dir.join("ax");
    
    // Download latest ax binary from GitHub releases
    run_command("curl", &[
        "-L",
        "https://github.com/manpreet113/ax/releases/latest/download/ax",
        "-o",
        ax_binary.to_str().unwrap()
    ])?;
    
    // Make executable and move to /usr/local/bin
    run_command("chmod", &["+x", ax_binary.to_str().unwrap()])?;
    run_command("sudo", &["mv", ax_binary.to_str().unwrap(), "/usr/local/bin/ax"])?;
    
    fs::remove_dir_all(&temp_dir).ok();
    println!("  ✓ ax installed/updated to latest version");
    
    // 2. System update
    println!("\n[Slate] Synchronizing repositories and updating system...");
    run_command("ax", &["-Syu", "--noconfirm"])?;
    
    // 3. Install all packages (official + AUR)
    println!("\n[Slate] Installing packages...");
    
    const PACKAGES: &[&str] = &[
        // Base system
        "base", "base-devel", "linux", "linux-firmware", "intel-ucode",
        // Boot & System
        "efibootmgr", "limine", "plymouth", "sudo",
        // Shell & CLI Tools
        "zsh", "bat", "eza", "fd", "zoxide", "starship", "jq", "less", "nano",
        // Hyprland & Wayland
        "hyprland", "hypridle", "hyprlock", "hyprpaper", "hyprlauncher", "hyprpolkitagent",
        "xdg-desktop-portal-hyprland", "waybar", "rofi", "mako",
        // Terminal & Apps
        "ghostty", "thunar", "code",
        // Audio & Video
        "pipewire", "pipewire-alsa", "pipewire-jack", "pipewire-pulse", "wireplumber",
        "gst-plugin-pipewire", "libpulse",
        // Graphics & Screenshot
        "grim", "slurp", "swappy",
        // Bluetooth & Network
        "bluez", "bluez-utils", "networkmanager", "wpa_supplicant",
        // Power & Hardware
        "brightnessctl", "power-profiles-daemon", "sof-firmware",
        // Printing
        "cups", "cups-pk-helper", "system-config-printer",
        // Fonts & Themes
        "ttf-iosevka-nerd", "ttf-jetbrains-mono-nerd", "terminus-font",
        "papirus-icon-theme", "nwg-look",
        // Utilities
        "git", "zram-generator",
        // AUR packages
        "wlogout", "zen-browser-bin", "clipse"
    ];
    
    let mut ax_args = vec!["-S", "--needed", "--noconfirm"];
    ax_args.extend(PACKAGES);
    run_command("ax", &ax_args)?;
    
    println!("  ✓ All packages installed");
    
    // 4. Install system configs
    println!("\n[Slate] Installing system configs...");
    let repo_dir = env::current_dir()?.canonicalize()?;
    let system_dir = repo_dir.join("system");
    
    if system_dir.exists() {
        // mkinitcpio.conf
        if system_dir.join("mkinitcpio.conf").exists() {
            run_command("sudo", &[
                "cp",
                system_dir.join("mkinitcpio.conf").to_str().unwrap(),
                "/etc/mkinitcpio.conf"
            ])?;
            println!("  ✓ Installed mkinitcpio.conf");
        }
        
        // Plymouth theme
        let plymouth_theme = system_dir.join("mono-steel");
        if plymouth_theme.exists() && plymouth_theme.is_dir() {
            let dest_dir = PathBuf::from("/usr/share/plymouth/themes/mono-steel");
            run_command("sudo", &["mkdir", "-p", dest_dir.to_str().unwrap()])?;
            
            // Recursively copy directory contents
            copy_dir_recursive_sudo(&plymouth_theme, &dest_dir)?;
            println!("  ✓ Installed Plymouth theme");
        }
    }
    
    // 6. Change default shell to zsh
    println!("\n[Slate] Verifying default shell...");
    let current_shell = env::var("SHELL").unwrap_or_default();
    
    if current_shell != "/usr/bin/zsh" {
        println!("  → Changing default shell to zsh");
        run_command("chsh", &["-s", "/usr/bin/zsh"])?;
        println!("  ✓ Default shell set to zsh");
    } else {
        println!("  ✓ zsh is already the default shell");
    }
    
    // 7. Detect hardware and patch bootloader
    println!("\n[Slate] Discovering hardware configuration...");
    let partuuid = detect_partuuid()?;
    println!("  ✓ Root PARTUUID: {}", partuuid);
    
    println!("\n[Slate] Patching bootloader...");
    patch_bootloader(&system_dir, &partuuid)?;
    
    // 8. Set Plymouth theme and rebuild initcpio
    println!("\n[Slate] Setting Plymouth theme and rebuilding initcpio...");
    run_command("sudo", &["plymouth-set-default-theme", "-R", "mono-steel"])?;
    println!("  ✓ Plymouth theme set and initcpio rebuilt");
    
    // 9. Run slate init to set up config management
    println!("\n[Slate] Initializing configuration management...");
    crate::commands::init()?;
    
    println!("\n[Slate] Installation complete!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Reboot to enter the void.");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    Ok(())
}

fn detect_partuuid() -> Result<String> {
    use crate::system;

    let root_device = system::get_root_device()
        .context("Failed to detect root device")?;
    
    if !root_device.starts_with("/dev/mapper/") && !root_device.starts_with("/dev/dm-") {
        bail!(
            "Slate requires LUKS encryption. Root device is {}, not a mapped device.",
            root_device
        );
    }
    
    let physical_device = system::trace_to_physical_partition(&root_device)
        .context("Failed to trace root device to physical partition")?;
        
    let partuuid = system::get_partuuid(&physical_device)
        .context("Failed to get PARTUUID")?;
        
    Ok(partuuid)
}

fn patch_bootloader(system_dir: &PathBuf, partuuid: &str) -> Result<()> {
    // Check for Limine
    if PathBuf::from("/boot/limine").exists() || PathBuf::from("/boot/limine.conf").exists() {
        println!("  → Detected Limine bootloader");
        let limine_conf = system_dir.join("limine.conf");
        
        if !limine_conf.exists() {
            println!("  ⚠ Warning: system/limine.conf not found, skipping limine patch");
            return Ok(());
        }
        
        run_command("sudo", &["mkdir", "-p", "/boot/limine"])?;
        
        // Read template, replace PARTUUID, write to /boot
        let template = fs::read_to_string(&limine_conf)?;
        let patched = template.replace("{{ROOT_PARTUUID}}", partuuid);
        
        let temp_file = std::env::temp_dir().join("limine.conf");
        fs::write(&temp_file, patched)?;
        
        run_command("sudo", &[
            "cp",
            temp_file.to_str().unwrap(),
            "/boot/limine/limine.conf"
        ])?;
        
        println!("  ✓ Patched /boot/limine/limine.conf");
        
    } else if PathBuf::from("/boot/loader/entries").exists() {
        println!("  → Detected systemd-boot");
        
        let entries_path = PathBuf::from("/boot/loader/entries");
        let arch_entry = fs::read_dir(&entries_path)?
            .filter_map(|e| e.ok())
            .find(|e| {
                e.file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains("arch")
            });
        
        if let Some(entry) = arch_entry {
            let entry_path = entry.path();
            println!("  → Patching {}", entry_path.display());
            
            let content = fs::read_to_string(&entry_path)?;
            let patched = regex::Regex::new(r"root=PARTUUID=[a-zA-Z0-9-]+")?
                .replace(&content, &format!("root=PARTUUID={}", partuuid));
            
            let temp_file = std::env::temp_dir().join("systemd-boot-entry.conf");
            fs::write(&temp_file, patched.as_ref())?;
            
            run_command("sudo", &[
                "cp",
                temp_file.to_str().unwrap(),
                entry_path.to_str().unwrap()
            ])?;
            
            println!("  ✓ Patched systemd-boot entry");
        } else {
            println!("  ⚠ Warning: No Arch entry found in /boot/loader/entries/");
        }
        
    } else {
        println!("  ⚠ Unknown bootloader. You'll need to configure boot parameters manually.");
    }
    
    Ok(())
}

fn copy_dir_recursive_sudo(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    // Ensure destination directory exists
    run_command("sudo", &["mkdir", "-p", dst.to_str().unwrap()])?;
    
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(&file_name);
        
        if src_path.is_dir() {
            // Recursively copy subdirectory
            copy_dir_recursive_sudo(&src_path, &dst_path)?;
        } else {
            // Copy file with sudo
            run_command("sudo", &[
                "cp",
                src_path.to_str().unwrap(),
                dst_path.to_str().unwrap()
            ])?;
        }
    }
    
    Ok(())
}

fn run_command(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .status()
        .with_context(|| format!("Failed to run: {} {}", cmd, args.join(" ")))?;
    
    if !status.success() {
        bail!("Command failed: {} {}", cmd, args.join(" "));
    }
    
    Ok(())
}
