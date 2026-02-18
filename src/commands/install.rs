use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn install() -> Result<()> {
    crate::system::ensure_not_root()?;
    crate::system::ensure_base_devel()?;

    println!("[Slate] Full system installation starting...");
    println!(
        "[Slate] This will install packages, configure bootloader, and set up system files.\n"
    );

    // 1. Ensure ax is up-to-date (always download latest)
    println!("[Slate] Installing/updating ax package manager...");

    let temp_dir = std::env::temp_dir().join("ax-install");
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)?;
    }
    fs::create_dir_all(&temp_dir)?;

    let ax_binary = temp_dir.join("ax");

    // Download latest ax binary from GitHub releases
    run_command(
        "curl",
        &[
            "-L",
            "https://github.com/manpreet113/ax/releases/latest/download/ax",
            "-o",
            ax_binary.to_str().unwrap(),
        ],
    )?;

    // Make executable and move to /usr/local/bin
    run_command("chmod", &["+x", ax_binary.to_str().unwrap()])?;
    run_command(
        "sudo",
        &["mv", ax_binary.to_str().unwrap(), "/usr/local/bin/ax"],
    )?;

    fs::remove_dir_all(&temp_dir).ok();
    println!("  ✓ ax installed/updated to latest version");

    // 2. System update
    println!("\n[Slate] Synchronizing repositories and updating system...");
    run_command("ax", &["-Syu", "--noconfirm"])?;

    // 3. Install all packages (official + AUR)
    println!("\n[Slate] Installing packages...");

    const PACKAGES: &[&str] = &[
        // Base system
        "base",
        "base-devel",
        "linux",
        "linux-firmware",
        "intel-ucode",
        // Boot & System (systemd-boot + UKI, no Limine/Plymouth)
        "efibootmgr",
        "systemd-ukify",
        "sudo",
        // Shell & CLI Tools
        "zsh",
        "bat",
        "eza",
        "fd",
        "zoxide",
        "starship",
        "jq",
        "less",
        "nano",
        // Hyprland & Wayland
        "hyprland",
        "hypridle",
        "hyprlock",
        "hyprpaper",
        "hyprlauncher",
        "hyprpolkitagent",
        "xdg-desktop-portal-hyprland",
        "waybar",
        "rofi",
        "mako",
        // Terminal & Apps
        "ghostty",
        "thunar",
        "code",
        // Audio & Video
        "pipewire",
        "pipewire-alsa",
        "pipewire-jack",
        "pipewire-pulse",
        "wireplumber",
        "gst-plugin-pipewire",
        "libpulse",
        // Graphics & Screenshot
        "grim",
        "slurp",
        "swappy",
        "wl-clipboard",
        // Theme generation
        "matugen",
        // Bluetooth & Network
        "bluez",
        "bluez-utils",
        "networkmanager",
        "wpa_supplicant",
        // Power & Hardware
        "brightnessctl",
        "power-profiles-daemon",
        "sof-firmware",
        // Printing
        "cups",
        "cups-pk-helper",
        "system-config-printer",
        // Fonts & Themes
        "ttf-iosevka-nerd",
        "ttf-jetbrains-mono-nerd",
        "terminus-font",
        "papirus-icon-theme",
        "nwg-look",
        // Utilities
        "git",
        "zram-generator",
        // AUR packages
        "wlogout",
        "zen-browser-bin",
        "clipse",
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
            run_command(
                "sudo",
                &[
                    "cp",
                    system_dir.join("mkinitcpio.conf").to_str().unwrap(),
                    "/etc/mkinitcpio.conf",
                ],
            )?;
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
    // 7. Run slate init to setup config and templates (Required for bootloader config)
    println!("\n[Slate] Initializing configuration management...");
    crate::commands::init()?;

    // 8. Configure systemd-boot with UKI (Depends on slate.toml from init)
    println!("\n[Slate] Configuring systemd-boot with UKI...");
    configure_systemd_boot()?;

    // 9. Install slate binary system-wide
    println!("\n[Slate] Installing slate binary...");
    let current_exe = std::env::current_exe().context("Failed to locate slate binary")?;
    run_command(
        "sudo",
        &["cp", current_exe.to_str().unwrap(), "/usr/local/bin/slate"],
    )?;
    run_command("sudo", &["chmod", "+x", "/usr/local/bin/slate"])?;
    println!("  ✓ Installed to /usr/local/bin/slate");

    // 10. Copy default wallpapers
    println!("\n[Slate] Installing default wallpapers...");
    let wall_dir = home::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
        .join("Pictures/Wallpapers");
    std::fs::create_dir_all(&wall_dir)?;
    let repo_walls = std::env::current_dir()?.join("wallpapers");
    if repo_walls.exists() {
        for entry in std::fs::read_dir(&repo_walls)? {
            let entry = entry?;
            let dest = wall_dir.join(entry.file_name());
            if !dest.exists() {
                std::fs::copy(entry.path(), &dest)?;
                println!("  ✓ {}", entry.file_name().to_string_lossy());
            }
        }
    } else {
        println!("  ⚠ No wallpapers/ directory found in repo");
    }

    println!("\n[Slate] Installation complete!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Reboot to enter the void.");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    Ok(())
}

fn copy_dir_recursive_sudo(src: &PathBuf, dst: &Path) -> Result<()> {
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
            run_command(
                "sudo",
                &["cp", src_path.to_str().unwrap(), dst_path.to_str().unwrap()],
            )?;
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

fn configure_systemd_boot() -> Result<()> {
    use crate::config::SlateConfig;
    use crate::template::TemplateEngine;

    //Load config to get PARTUUID
    let home = home::home_dir().context("Could not determine home directory")?;
    let config_path = home.join(".config/slate/slate.toml");
    let templates_dir = home.join(".config/slate/templates");

    let config = SlateConfig::load(&config_path)?;
    let engine = TemplateEngine::new(templates_dir.to_str().unwrap())?;

    // Step 1: Render and write systemd templates
    println!("  → Writing kernel cmdline...");
    let cmdline_content = engine.render("systemd/slate.conf", &config)?;
    run_command("sudo", &["mkdir", "-p", "/etc/cmdline.d"])?;
    write_with_sudo("/etc/cmdline.d/slate.conf", &cmdline_content)?;

    println!("  → Writing mkinitcpio config...");
    let mkinitcpio_content = engine.render("systemd/mkinitcpio.conf", &config)?;
    write_with_sudo("/etc/mkinitcpio.conf", &mkinitcpio_content)?;

    println!("  → Writing linux preset...");
    let preset_content = engine.render("systemd/linux.preset", &config)?;
    run_command("sudo", &["mkdir", "-p", "/etc/mkinitcpio.d"])?;
    write_with_sudo("/etc/mkinitcpio.d/linux.preset", &preset_content)?;

    // Step 2: Build UKI (mkinitcpio will invoke ukify due to preset)
    println!("  → Building Unified Kernel Image...");
    run_command("sudo", &["mkinitcpio", "-p", "linux"])?;

    // Step 3: Install systemd-boot (auto-discovers slate.efi)
    println!("  → Installing systemd-boot...");
    run_command("sudo", &["bootctl", "install"])?;

    println!("  ✓ systemd-boot configured with encrypted UKI");
    Ok(())
}

fn write_with_sudo(path: &str, content: &str) -> Result<()> {
    let temp_file = std::env::temp_dir().join(format!("slate-{}", std::process::id()));
    fs::write(&temp_file, content)?;
    run_command("sudo", &["cp", temp_file.to_str().unwrap(), path])?;
    fs::remove_file(&temp_file).ok();
    Ok(())
}
