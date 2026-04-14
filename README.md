# SLATE 🪐

Slate is a semi-automated Arch Linux installer built with **Ratatui**. It provisions a full Arch system plus the **Slate shell**: a Hyprland + Quickshell desktop profile deployed automatically during installation.

![TUI Demo](https://img.shields.io/badge/TUI-Premium-blueviolet)
![Rust](https://img.shields.io/badge/Language-Rust-orange)
![License](https://img.shields.io/badge/License-MIT-green)

## Current Status
Slate currently handles:
- Interactive disk selection and multi-step configuration forms.
- Zero-typing automatic partitioning (1GB EFI + remaining Btrfs).
- Automated Btrfs subvolume layout (`@`, `@home`, `@log`, `@pkg`, `@snapshots`).
- Bootloader setup and `ax` tool installation.
- Automatic Slate shell provisioning from the upstream shell repo, including package installation and Hyprland shell config deployment.

## Usage

1. Boot into an Arch Linux Live ISO.
2. Download the latest pre-release binary:
   ```bash
   curl -sL https://github.com/manpreet113/slate/releases/download/latest/slate -o slate
   chmod +x slate
   ```
3. Run the installer:
   ```bash
   sudo ./slate install
   ```
4. Follow the TUI prompts to configure your hostname, user, keymap, and select your target disk.
5. Let Slate finish chroot provisioning; it will clone the Slate shell repo, install the desktop packages through `ax`, and deploy the shell files automatically.

## Development

Slate is written in Rust. To build from source:

```bash
cargo build --release
```

## License

Released under the MIT License.
