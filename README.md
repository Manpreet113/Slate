# Slate

**Slate** is a modern, opinionated Arch Linux installer and configuration manager.

It transforms a raw machine into a fully configured Arch Linux workstation with Hyprland, complete with a custom design system, secret management, and immutable-style config generation.

## Features

- **Forge Installer**: A single binary that partitions, encrypts (LUKS2), formats (Btrfs), and bootstraps Arch Linux.
- **Config Management**: Generates config files for Hyprland, Waybar, Alacritty, and more from a central `slate.toml`.
- **Theme Engine**: Integrated `matugen` support to generate color palettes from wallpapers.
- **Secret Management**: Encrypts sensitive keys using `age` (via `rtoolbox`).
- **Declarative**: Define your system state in one file.

## Installation

Boot into the Arch Linux Live ISO, connect to the internet, and run:

```bash
# 1. Download Slate
curl -L https://github.com/manpreet113/slate/releases/latest/download/slate -o slate
chmod +x slate

# 2. Run the Forge
# REPLACE /dev/nvme0n1 with your target disk!
./slate forge /dev/nvme0n1
```

**What `slate forge` does:**
1.  **Preflight**: Checks for UEFI, internet, and root privileges.
2.  **Cleansing**: Wipes the disk and creates partitions (EFI + LUKS Root).
3.  **Vault**: Encrypts the root partition with LUKS2.
4.  **Subvolumes**: Creates Btrfs subvolumes (`@`, `@home`, `@pkg`).
5.  **Injection**: Bootstraps the base system and installs `slate` and `ax`.
6.  **Chroot Stage**: Automatically enters the new system to:
    - Set timezone/locale.
    - Create your user.
    - Set up systemd-boot with UKI.
    - Install packages via `ax`.
    - Deploy configs.

## Usage

### Post-Install

After rebooting into your new system:

```bash
# Initialize user configuration (if not done automatically)
slate init

# Reload configurations after editing slate.toml
slate reload
```

### Wallpaper & Theming

Set a new wallpaper and automatically generate a color scheme:

```bash
slate wall ~/Pictures/Wallpapers/mountain.jpg
```

This will:
- Copy the wallpaper to `~/Pictures/Wallpapers/`.
- Generate a Material You palette using `matugen`.
- Update `slate.toml`.
- Reload all applications (Hyprland, Waybar, etc.) with the new colors.

## Configuration

Your system is defined in `~/.config/slate/slate.toml`.

```toml
[hardware]
host = "arch-desktop"
font_family = "JetBrains Mono"

[palette]
mode = "matugen"
accent = "#ff0000" # Fallback if matugen fails
```

Templates are stored in `~/.config/slate/templates/` and use the Tera templating engine.

## Development

```bash
# Build locally
cargo build --release

# Run checks
cargo fmt --check
cargo check
cargo clippy
```