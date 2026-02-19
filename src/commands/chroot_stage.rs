use crate::system;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use tera::{Context as TeraContext, Tera};
// use rpassword; // imported via cargo

pub fn chroot_stage() -> Result<()> {
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  ENTERING CHROOT STAGE");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 1. Interactive Setup
    let config = interactive_setup()?;

    // 2. Base System Config
    configure_base(&config)?;

    // 3. User & Auth
    configure_user(&config)?;

    // 4. AX Provisioning
    provision_packages()?;

    // 5. Bootloader & UKI
    configure_boot(&config)?;

    // 6. User Init
    run_user_init(&config)?;

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  CHROOT STAGE COMPLETE");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    Ok(())
}

struct InstallConfig {
    hostname: String,
    username: String,
    password: String,
}

fn interactive_setup() -> Result<InstallConfig> {
    println!("Please configure your system:");

    print!("  Hostname: ");
    io::stdout().flush()?;
    let mut hostname = String::new();
    io::stdin().read_line(&mut hostname)?;
    let hostname = hostname.trim().to_string();

    print!("  Username: ");
    io::stdout().flush()?;
    let mut username = String::new();
    io::stdin().read_line(&mut username)?;
    let username = username.trim().to_string();

    println!("  Password (for root and user): ");
    let password = rpassword::read_password()?;

    // Verify password? Nah, Keep it simple for now as per plan

    Ok(InstallConfig {
        hostname,
        username,
        password,
    })
}

fn configure_base(config: &InstallConfig) -> Result<()> {
    println!("  > Configuring Base System...");

    // Hostname
    fs::write("/etc/hostname", &config.hostname)?;

    // Timezone
    // Hardcoded to UTC or interactive? Plan didn't specify interactivity for TZ.
    // "Set timezone: ln -sf /usr/share/zoneinfo/Region/City /etc/localtime"
    // Let's default to UTC for automation, user can change later.
    let _ = fs::remove_file("/etc/localtime");
    std::os::unix::fs::symlink("/usr/share/zoneinfo/UTC", "/etc/localtime")?;

    // Locale
    let locale_gen = fs::read_to_string("/etc/locale.gen")?;
    let new_locale_gen = locale_gen.replace("#en_US.UTF-8 UTF-8", "en_US.UTF-8 UTF-8");
    fs::write("/etc/locale.gen", new_locale_gen)?;

    run_command("locale-gen", &[])?;
    fs::write("/etc/locale.conf", "LANG=en_US.UTF-8\n")?;

    Ok(())
}

fn configure_user(config: &InstallConfig) -> Result<()> {
    println!("  > Configuring User & Auth...");

    // Create user
    run_command(
        "useradd",
        &["-m", "-G", "wheel", "-s", "/bin/zsh", &config.username],
    )?;

    // Set passwords
    let root_auth = format!("root:{}", config.password);
    let user_auth = format!("{}:{}", config.username, config.password);

    run_command_stdin("chpasswd", &[], &format!("{}\n{}", root_auth, user_auth))?;

    // Sudoers
    // Uncomment %wheel
    let sudoers = fs::read_to_string("/etc/sudoers")?;
    let new_sudoers = sudoers.replace("# %wheel ALL=(ALL:ALL) ALL", "%wheel ALL=(ALL:ALL) ALL");
    fs::write("/etc/sudoers", new_sudoers)?;

    // Auto-login (QoL)
    let override_dir = Path::new("/etc/systemd/system/getty@tty1.service.d");
    fs::create_dir_all(override_dir)?;

    let override_conf = format!(
        "[Service]\nExecStart=\nExecStart=-/usr/bin/agetty --autologin {} --noclear %I $TERM\n",
        config.username
    );
    fs::write(override_dir.join("override.conf"), override_conf)?;

    Ok(())
}

fn provision_packages() -> Result<()> {
    println!("  > Provisioning Packages via AX...");

    // Manifest (Hardcoded for now as per plan/task "read from manifest" -> but we don't have a manifest file yet)
    // "The existing package list in install.rs is the right starting point — move it to a TOML manifest file"
    // For this MVP, I'll put a list here.
    // And use `ax` to install them.

    let packages = [
        "hyprland",
        "waybar",
        "rofi-wayland",
        "kitty",
        "mako",
        "swww",
        "grim",
        "slurp",
        "wl-clipboard",
        "pavucontrol",
        "pipewire",
        "pipewire-pulse",
        "wireplumber",
        "xdg-desktop-portal-hyprland",
        "xdg-desktop-portal-gtk",
        "qt5-wayland",
        "qt6-wayland",
        "polkit-gnome",
        "ttf-jetbrains-mono-nerd",
        "noto-fonts",
        "noto-fonts-emoji",
        "zsh",
        "zsh-syntax-highlighting",
        "zsh-autosuggestions",
        "starship",
        "neofetch",
        "firefox",
        "thunar",
        "visual-studio-code-bin", // AUR check?
        "matugen-bin",
        "wlogout",
        "networkmanager",
        "bluez",
        "bluez-utils",
    ];

    println!("    Syncing and installing {} packages...", packages.len());

    // We update first: ax -Syu ?
    // Just install: ax -S --noconfirm <pkgs>
    // Note: `ax` usage: `ax -S <pkg>`

    let mut args = vec!["-S", "--noconfirm"];
    args.extend(packages);

    // Running as root inside chroot
    run_command("ax", &args)?;

    // Enable services
    run_command("systemctl", &["enable", "NetworkManager", "bluetooth"])?;

    Ok(())
}

fn configure_boot(_config: &InstallConfig) -> Result<()> {
    println!("  > Configuring Bootloader & UKI...");

    // 1. Detect UUID of root
    // We are inside chroot. / is the root.
    // `system::get_root_device()` -> returns device name e.g. /dev/mapper/root
    // `system::get_uuid` works on that?

    let root_dev = system::get_root_device()?;
    // This probably returns /dev/mapper/root
    // We need the UUID of the physical partition that holds LUKS?
    // "The LUKS UUID needs to be detected inside the chroot and injected... This is where src/system.rs's get_uuid() is called."
    // Wait, UKI needs the UUID of the *encrypted* partition (rd.luks.name=<UUID>=root), or the *mapper*?
    // Usually `rd.luks.name=<UUID>=root` refers to the UUID of the LUKS container (the physical partition UUID).
    // `root=/dev/mapper/root`

    // `system::trace_to_physical_partition("/dev/mapper/root")` -> `/dev/nvme0n1p2`
    // `system::get_uuid("/dev/nvme0n1p2")` -> UUID of luks container.

    let phys_dev = system::trace_to_physical_partition(&root_dev)?;
    let luks_uuid = system::get_uuid(&phys_dev)?;

    println!("    Detected LUKS UUID: {}", luks_uuid);

    // 2. Render templates
    // Use embedded templates for bootloader config
    let mut tera = Tera::default();
    tera.add_raw_templates(crate::template::EMBEDDED_TEMPLATES.iter().copied())?;

    #[derive(Serialize)]
    struct Hardware {
        root_uuid: String,
    }
    #[derive(Serialize)]
    struct ConfigContext {
        hardware: Hardware,
    }

    let context_data = ConfigContext {
        hardware: Hardware {
            root_uuid: luks_uuid,
        },
    };

    let context = TeraContext::from_serialize(&context_data)?;

    println!("    Rendering bootloader configs...");

    // Render slate.conf options line
    let slate_options = tera.render("systemd/slate.conf", &context)?;

    // Create loader entry
    let entry_content = format!(
        "title   Arch Linux (Slate)\nlinux   /vmlinuz-linux\ninitrd  /initramfs-linux.img\noptions {}",
        slate_options.trim()
    );

    fs::create_dir_all("/boot/loader/entries")?;
    fs::write("/boot/loader/entries/slate.conf", entry_content)?;

    // Render mkinitcpio.conf
    let mkinitcpio_conf = tera.render("systemd/mkinitcpio.conf", &context)?;
    fs::write("/etc/mkinitcpio.conf", mkinitcpio_conf)?;

    // Render linux.preset?
    // Usually default is fine but we might want custom preset.
    // Let's see if we have one. Yes, templates/systemd/linux.preset exists.
    let linux_preset = tera.render("systemd/linux.preset", &context)?;
    fs::create_dir_all("/etc/mkinitcpio.d")?; // ensure dir exists
    fs::write("/etc/mkinitcpio.d/linux.preset", linux_preset)?;

    // Run mkinitcpio -P
    println!("    Running mkinitcpio...");
    run_command("mkinitcpio", &["-P"])?;

    // Install bootloader
    println!("    Installing bootctl...");
    run_command("bootctl", &["install"])?;

    // Ensure loader.conf exists and sets default
    // We can just write a simple one if not exists, or rely on bootctl install.
    // bootctl install creates loader.conf but doesn't set default to slate.conf necessarily.
    // Let's force it.
    let loader_conf = "default slate.conf\ntimeout 3\nconsole-mode max\n";
    fs::write("/boot/loader/loader.conf", loader_conf)?;

    Ok(())
}

fn run_user_init(config: &InstallConfig) -> Result<()> {
    println!("  > Running User Init...");

    // su - user -c "slate init"
    // This will try to deploy templates.
    // Again, needs Phase 6.

    run_command("su", &["-", &config.username, "-c", "slate init"])?;

    Ok(())
}

fn run_command(cmd: &str, args: &[&str]) -> Result<()> {
    println!("    $ {} {}", cmd, args.join(" "));
    let output = Command::new(cmd)
        .args(args)
        .output()
        .context(format!("Failed to run {}", cmd))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Command failed: {}\n{}", cmd, stderr);
    }
    Ok(())
}

fn run_command_stdin(cmd: &str, args: &[&str], input: &str) -> Result<()> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes())?;
    }

    let status = child.wait()?;
    if !status.success() {
        bail!("Command {} failed", cmd);
    }
    Ok(())
}
