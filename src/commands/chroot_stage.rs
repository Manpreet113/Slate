use crate::system;
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use crate::tui::UserInfo;

pub fn chroot_stage() -> Result<()> {
    // 1. Load User Info
    let user_info_path = Path::new("/root/user_info.json");
    let user_info_content = fs::read_to_string(user_info_path)
        .context("Failed to read user_info.json in chroot")?;
    let user_info: UserInfo = serde_json::from_str(&user_info_content)?;

    // 2. Base System Config
    configure_base(&user_info)?;

    // 3. User & Auth & Shell
    configure_user(&user_info)?;

    // 4. Desktop Environment (Direct Boot & Hyprland)
    configure_desktop(&user_info)?;

    // 4.5 Shell UI (Quickshell-based)
    configure_shell_ui(&user_info)?;

    // 5. Tooling (Ax, VSCode, Clipse, Git)
    configure_tools(&user_info)?;

    // 6. Post-config Services
    configure_post_services()?;

    // 7. Bootloader
    configure_boot()?;

    Ok(())
}

fn configure_base(config: &UserInfo) -> Result<()> {
    println!("  > Configuring Base System...");

    enable_multilib_repo()?;

    // Hostname
    fs::write("/etc/hostname", format!("{}\n", config.hostname))?;

    // Timezone
    let _ = fs::remove_file("/etc/localtime");
    let zone_path = format!("/usr/share/zoneinfo/{}", config.timezone);
    if Path::new(&zone_path).exists() {
        let _ = std::os::unix::fs::symlink(&zone_path, "/etc/localtime");
    } else {
        let _ = std::os::unix::fs::symlink("/usr/share/zoneinfo/UTC", "/etc/localtime");
    }

    // Locale
    let locale_gen = "/etc/locale.gen";
    if Path::new(locale_gen).exists() {
        let content = fs::read_to_string(locale_gen)?;
        let new_content = content.replace("#en_US.UTF-8 UTF-8", "en_US.UTF-8 UTF-8");
        fs::write(locale_gen, new_content)?;
        run_command("locale-gen", &[])?;
    }
    fs::write("/etc/locale.conf", "LANG=en_US.UTF-8\n")?;

    // Keymap
    fs::write("/etc/vconsole.conf", format!("KEYMAP={}\n", config.keymap))?;

    // Time & NTP
    run_command("hwclock", &["--systohc"])?;

    Ok(())
}

fn enable_multilib_repo() -> Result<()> {
    let pacman_conf = "/etc/pacman.conf";
    if !Path::new(pacman_conf).exists() {
        return Ok(());
    }

    let content = fs::read_to_string(pacman_conf)?;
    if content.contains("\n[multilib]\n") && content.contains("\nInclude = /etc/pacman.d/mirrorlist\n") {
        return Ok(());
    }

    // Enable only the [multilib] block include, not any Include line in [options].
    let mut updated_lines: Vec<String> = Vec::new();
    let mut in_multilib = false;
    let mut saw_multilib = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "#[multilib]" {
            updated_lines.push("[multilib]".to_string());
            in_multilib = true;
            saw_multilib = true;
            continue;
        }
        if trimmed == "[multilib]" {
            updated_lines.push(line.to_string());
            in_multilib = true;
            saw_multilib = true;
            continue;
        }

        if trimmed.starts_with('[') && trimmed != "[multilib]" {
            in_multilib = false;
        }

        if in_multilib && trimmed == "#Include = /etc/pacman.d/mirrorlist" {
            updated_lines.push("Include = /etc/pacman.d/mirrorlist".to_string());
            continue;
        }

        updated_lines.push(line.to_string());
    }

    let mut updated = updated_lines.join("\n");
    if content.ends_with('\n') {
        updated.push('\n');
    }

    if !saw_multilib {
        if !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push_str("\n[multilib]\nInclude = /etc/pacman.d/mirrorlist\n");
    }

    if updated != content {
        fs::write(pacman_conf, updated)?;
        run_command("pacman", &["-Sy", "--noconfirm"])?;
    }

    Ok(())
}

fn configure_user(config: &UserInfo) -> Result<()> {
    println!("  > Configuring User & Zsh...");

    // Create user with Zsh
    run_command("useradd", &["-m", "-G", "wheel", "-s", "/usr/bin/zsh", &config.username])?;

    // Set passwords
    let root_auth = format!("root:{}", config.password);
    let user_auth = format!("{}:{}", config.username, config.password);
    run_command_stdin("chpasswd", &[], &format!("{}\n{}", root_auth, user_auth))?;

    // Sudoers
    let sudoers_file = "/etc/sudoers";
    if Path::new(sudoers_file).exists() {
        let sudoers = fs::read_to_string(sudoers_file)?;
        let new_sudoers = sudoers.replace("# %wheel ALL=(ALL:ALL) ALL", "%wheel ALL=(ALL:ALL) ALL");
        fs::write(sudoers_file, new_sudoers)?;
    }

    // Modern Zshrc
    let zshrc = r#"
# slate-desktop: modern zshrc
alias ls='eza --icons'
alias l='eza -lh --icons'
alias ll='eza -lha --icons'
alias cat='bat'
alias grep='rg'
alias cd='zoxide'

# Starship Prompt
eval "$(starship init zsh)"
eval "$(zoxide init zsh)"

# Path
export PATH=$PATH:$HOME/.local/bin
"#;
    let user_home = format!("/home/{}", config.username);
    fs::write(format!("{}/.zshrc", user_home), zshrc)?;
    
    // Auto-login to TTY1
    fs::create_dir_all("/etc/systemd/system/getty@tty1.service.d")?;
    let autologin_override = format!(r#"[Service]
ExecStart=
ExecStart=-/usr/bin/agetty --autologin {} --noclear %I $TERM
"#, config.username);
    fs::write("/etc/systemd/system/getty@tty1.service.d/override.conf", autologin_override)?;

    // Auto-start Hyprland via .zprofile
    let zprofile = r#"
# slate-desktop: auto-start hyprland on tty1
if [[ -z $DISPLAY ]] && [[ $(tty) = /dev/tty1 ]]; then
  exec start-hyprland
fi
"#;
    fs::write(format!("{}/.zprofile", user_home), zprofile)?;
    
    // Starship config (minimal)
    fs::create_dir_all(format!("{}/.config", user_home))?;
    fs::write(format!("{}/.config/starship.toml", user_home), "[add_newline]\ninsert_newline = false\n")?;

    run_command("chown", &["-R", &format!("{}:{}", config.username, config.username), &user_home])?;

    Ok(())
}

fn configure_desktop(config: &UserInfo) -> Result<()> {
    println!("  > Configuring Modular Hyprland...");

    let user_home = format!("/home/{}", config.username);
    let hypr_dir = format!("{}/.config/hypr", user_home);
    fs::create_dir_all(&hypr_dir)?;

    // 1. hyprland.conf (Main Loader)
    let hypr_main = r#"
# slate-desktop: modular hyprland config
source = ~/.config/hypr/monitors.conf
source = ~/.config/hypr/autostart.conf
source = ~/.config/hypr/input.conf
source = ~/.config/hypr/appearance.conf
source = ~/.config/hypr/animations.conf
source = ~/.config/hypr/keybinds.conf
source = ~/.config/hypr/windowrules.conf

$terminal = kitty
$browser = firefox
"#;
    fs::write(format!("{}/hyprland.conf", hypr_dir), hypr_main)?;

    // 2. monitors.conf
    let hypr_monitors = r#"
# Displays
monitor=,preferred,auto,auto
"#;
    fs::write(format!("{}/monitors.conf", hypr_dir), hypr_monitors)?;

    // 3. autostart.conf
    let hypr_autostart = r#"
# Security: start hyprlock immediately
exec-once = hyprlock

# Core Services
exec-once = clipse -listen
"#;
    fs::write(format!("{}/autostart.conf", hypr_dir), hypr_autostart)?;

    // 4. input.conf
    let hypr_input = format!(r#"
input {{
    kb_layout = {}
    follow_mouse = 1
    touchpad {{
        natural_scroll = yes
    }}
}}
"#, config.keymap);
    fs::write(format!("{}/input.conf", hypr_dir), hypr_input)?;

    // 5. appearance.conf
    let hypr_appearance = r#"
general {
    gaps_in = 5
    gaps_out = 10
    border_size = 2
    col.active_border = rgba(33ccffee) rgba(00ff99ee) 45deg
    col.inactive_border = rgba(595959aa)
    layout = dwindle
}

decoration {
    rounding = 10
    blur {
        enabled = true
        size = 8
        passes = 2
        new_optimizations = on
    }
    drop_shadow = yes
    shadow_range = 10
    shadow_render_power = 3
    col.shadow = rgba(1a1a1aee)
}
"#;
    fs::write(format!("{}/appearance.conf", hypr_dir), hypr_appearance)?;

    // 6. animations.conf
    let hypr_animations = r#"
animations {
    enabled = true
    bezier = myBezier, 0.05, 0.9, 0.1, 1.05
    animation = windows, 1, 7, myBezier
    animation = windowsOut, 1, 7, default, popin 80%
    animation = border, 1, 10, default
    animation = fade, 1, 7, default
    animation = workspaces, 1, 6, default
}
"#;
    fs::write(format!("{}/animations.conf", hypr_dir), hypr_animations)?;

    // 7. keybinds.conf
    let hypr_keybinds = r#"
# Core Binds
bind = SUPER, Return, exec, $terminal
bind = SUPER, B, exec, $browser
bind = SUPER, Q, killactive,
bind = SUPER, M, exit,
bind = SUPER, F, togglefloating,
bind = SUPER, Space, exec, wofi --show drun
bind = SUPER, V, exec, kitty --title clipse -e clipse

# Window Focus
bind = SUPER, left, movefocus, l
bind = SUPER, right, movefocus, r
bind = SUPER, up, movefocus, u
bind = SUPER, down, movefocus, d

# Workspaces
bind = SUPER, 1, workspace, 1
bind = SUPER, 2, workspace, 2
bind = SUPER, 3, workspace, 3
bind = SUPER, 4, workspace, 4
bind = SUPER, 5, workspace, 5
bind = SUPER, 6, workspace, 6

bind = SUPER SHIFT, 1, movetoworkspace, 1
bind = SUPER SHIFT, 2, movetoworkspace, 2
bind = SUPER SHIFT, 3, movetoworkspace, 3
bind = SUPER SHIFT, 4, movetoworkspace, 4

# Mouse bindings
bindm = SUPER, mouse:272, movewindow
bindm = SUPER, mouse:273, resizewindow
"#;
    fs::write(format!("{}/keybinds.conf", hypr_dir), hypr_keybinds)?;

    // 8. windowrules.conf
    let hypr_windowrules = r#"
# Floating Dialogs
windowrule = float, file_progress
windowrule = float, confirm
windowrule = float, dialog
windowrule = float, download
windowrule = float, notification
windowrule = float, error
windowrule = float, splash
windowrule = float, confirmreset
windowrule = float, title:Open File
windowrule = float, title:branchdialog
windowrule = float, Lxappearance
windowrule = float, pavucontrol-qt
windowrule = float, pavucontrol
windowrule = float, file-roller
windowrule = float, title:wlogout

# Opacity Rules
windowrulev2 = opacity 0.9 0.9,class:^(kitty)$
windowrulev2 = opacity 0.95 0.9,class:^(firefox)$

# Clipse Floating setup (requires title param in bind)
windowrulev2 = float,class:(kitty),title:(clipse)
windowrulev2 = size 800 600,class:(kitty),title:(clipse)
"#;
    fs::write(format!("{}/windowrules.conf", hypr_dir), hypr_windowrules)?;

    run_command("chown", &["-R", &format!("{}:{}", config.username, config.username), &hypr_dir])?;

    Ok(())
}

fn configure_tools(config: &UserInfo) -> Result<()> {
    println!("[Phase 2/3] Ax user packages");
    println!("  > Finalizing Tools (Ax, Git, VSCode, Clipse)...");

    // Git Config
    if !config.git_name.is_empty() {
        let user_home = format!("/home/{}", config.username);
        let gitconfig = format!(r#"[user]
	name = {}
	email = {}
"#, config.git_name, config.git_email);
        fs::write(format!("{}/.gitconfig", user_home), gitconfig)?;
        run_command("chown", &[&format!("{}:{}", config.username, config.username), &format!("{}/.gitconfig", user_home)])?;
    }

    // AUR Packages via Ax
    println!("  > Installing desktop/tooling packages via Ax as user...");
    let packages = [
        // Desktop/session
        "hyprland",
        "hyprlock",
        "hypridle",
        "xdg-desktop-portal-hyprland",
        "qt6-wayland",
        // Audio/media
        "pipewire",
        "wireplumber",
        "pipewire-pulse",
        "pipewire-alsa",
        // Apps and CLI tools
        "firefox",
        "kitty",
        "wofi",
        "starship",
        "eza",
        "bat",
        "zoxide",
        "fzf",
        "ripgrep",
        // Utilities and extras
        "network-manager-applet",
        "blueman",
        "pavucontrol",
        "easyeffects",
        "grim",
        "slurp",
        "imagemagick",
        "jq",
        "sqlite",
        "upower",
        "wl-clipboard",
        "wlsunset",
        "wtype",
        "zbar",
        "glib2",
        "zenity",
        "power-profiles-daemon",
        // Fonts and AUR tooling packages
        "ttf-roboto",
        "ttf-roboto-mono",
        "ttf-dejavu",
        "ttf-liberation",
        "noto-fonts",
        "noto-fonts-cjk",
        "noto-fonts-emoji",
        "ttf-nerd-fonts-symbols",
        "gpu-screen-recorder",
        "adw-gtk-theme",
        "visual-studio-code-bin",
        "clipse",
    ];

    let mut pkg_vec = packages.to_vec();
    if config.shell_ui.is_some() {
        pkg_vec.push("quickshell-git");
        if config.shell_ui == Some("caelestia".to_string()) {
            pkg_vec.push("fish");
            pkg_vec.push("wget");
        }
    }

    let sudoers_dropin = format!("/etc/sudoers.d/90-slate-ax-{}", config.username);
    let sudoers_content = format!("{} ALL=(ALL:ALL) NOPASSWD: ALL\n", config.username);
    fs::write(&sudoers_dropin, sudoers_content)?;
    run_command("chmod", &["0440", &sudoers_dropin])?;

    // Force non-interactive sudo behavior in the spawned user shell.
    // This avoids hanging on password prompts when running under arch-chroot.
    let ax_cmd = format!("SUDO_ASKPASS=/bin/false ax -S {} --noconfirm", pkg_vec.join(" "));
    let install_res = run_command("su", &["-", &config.username, "-c", &ax_cmd]);

    let _ = fs::remove_file(&sudoers_dropin);
    install_res?;

    Ok(())
}

fn configure_post_services() -> Result<()> {
    println!("[Phase 3/3] post-config services");
    println!("  > Enabling core services...");
    run_command("systemctl", &["enable", "systemd-timesyncd"])?;
    run_command("systemctl", &["enable", "NetworkManager"])?;
    run_command("systemctl", &["enable", "bluetooth"])?;
    Ok(())
}

fn configure_boot() -> Result<()> {
    println!("  > Configuring Bootloader...");

    let root_dev = system::get_root_device()?;
    let root_uuid = system::get_uuid(&root_dev)?;

    run_command("bootctl", &["install"])?;

    let entry_content = format!(
        "title   Slate OS (Arch)\nlinux   /vmlinuz-linux\ninitrd  /intel-ucode.img\ninitrd  /amd-ucode.img\ninitrd  /initramfs-linux.img\noptions root=UUID={} rw rootflags=subvol=@\n",
        root_uuid
    );
    fs::create_dir_all("/boot/loader/entries")?;
    fs::write("/boot/loader/entries/arch.conf", entry_content)?;

    let loader_conf = "default arch.conf\ntimeout 3\nconsole-mode max\n";
    fs::write("/boot/loader/loader.conf", loader_conf)?;

    Ok(())
}

fn run_command(cmd: &str, args: &[&str]) -> Result<()> {
    println!("    $ {} {}", cmd, args.join(" "));
    let output = Command::new(cmd)
        .args(args)
        .output()
        .context(format!("Failed to run {}", cmd))?;

    if !output.status.success() {
        let stderr = sanitize_for_tui(&String::from_utf8_lossy(&output.stderr));
        let stderr = stderr.trim().to_string();
        if stderr.is_empty() {
            bail!("Command failed: {}", cmd);
        }
        bail!("Command failed: {}: {}", cmd, stderr);
    }
    Ok(())
}

fn run_command_stdin(cmd: &str, args: &[&str], input: &str) -> Result<()> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = sanitize_for_tui(&String::from_utf8_lossy(&output.stderr));
        let stderr = stderr.trim().to_string();
        if stderr.is_empty() {
            bail!("Command {} failed", cmd);
        }
        bail!("Command {} failed: {}", cmd, stderr);
    }
    Ok(())
}

fn sanitize_for_tui(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        // Strip ANSI escape sequences (CSI and a few common OSC forms).
        if ch == '\u{1b}' {
            if let Some('[') = chars.peek().copied() {
                let _ = chars.next();
                while let Some(next) = chars.next() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
                continue;
            }
            if let Some(']') = chars.peek().copied() {
                let _ = chars.next();
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                }
                continue;
            }
            continue;
        }

        // Drop control chars that can break layout, keep tabs/spaces.
        if ch == '\r' || (ch.is_control() && ch != '\n' && ch != '\t') {
            continue;
        }

        out.push(ch);
    }

    out
}

fn configure_shell_ui(config: &UserInfo) -> Result<()> {
    let ui_name = match &config.shell_ui {
        Some(name) => name,
        None => return Ok(()),
    };

    println!("  > Installing Shell UI: {}...", ui_name);
    let user_home = format!("/home/{}", config.username);

    match ui_name.as_str() {
        "ambxst" => {
            println!("    $ curl -L get.axeni.de/ambxst | sh");
            let cmd = "curl -L get.axeni.de/ambxst | sh";
            run_command("su", &["-", &config.username, "-c", cmd])?;

            // Update Hyprland autostart.conf
            let autostart_path = format!("{}/.config/hypr/autostart.conf", user_home);
            if Path::new(&autostart_path).exists() {
                let mut content = fs::read_to_string(&autostart_path)?;
                content.push_str("\n# Shell UI: Ambxst\nexec-once = ambxst\n");
                fs::write(&autostart_path, content)?;
            }
        }
        "caelestia" => {
            // Change shell to fish
            run_command("chsh", &["-s", "/usr/bin/fish", &config.username])?;

            // Clone and run install script
            let target_dir = format!("{}/.local/share/caelestia", user_home);
            fs::create_dir_all(&target_dir)?;
            run_command("chown", &["-R", &format!("{}:{}", config.username, config.username), &format!("{}/.local", user_home)])?;

            run_command("su", &["-", &config.username, "-c", "git clone https://github.com/caelestia-dots/caelestia.git ~/.local/share/caelestia"])?;
            run_command("su", &["-", &config.username, "-c", "fish ~/.local/share/caelestia/install.fish"])?;
        }
        "dank-material" => {
            println!("    $ curl -fsSL https://install.danklinux.com | sh");
            let cmd = "curl -fsSL https://install.danklinux.com | sh";
            run_command("su", &["-", &config.username, "-c", cmd])?;
        }
        _ => {}
    }

    run_command("chown", &["-R", &format!("{}:{}", config.username, config.username), &user_home])?;

    Ok(())
}
