# SLATE 🪐

Slate is currently a semi-automated Arch Linux OS installer built with **Ratatui**. It provides a premium, full-screen Terminal User Interface (TUI) to streamline setting up a clean, optimized Arch Linux system with **Btrfs** and **systemd-boot**.

![TUI Demo](https://img.shields.io/badge/TUI-Premium-blueviolet)
![Rust](https://img.shields.io/badge/Language-Rust-orange)
![License](https://img.shields.io/badge/License-MIT-green)

## Current Status
As of right now, Slate primarily functions as an Arch Linux installer. It handles:
- Interactive disk selection and multi-step configuration forms.
- Zero-typing automatic partitioning (1GB EFI + remaining Btrfs).
- Automated Btrfs subvolume layout (`@`, `@home`, `@log`, `@pkg`, `@snapshots`).
- Bootloader setup and `ax` tool installation.
- Native injection of the custom **Elysium Shell** desktop environment.

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

## Development

Slate is written in Rust. To build from source:

```bash
cargo build --release
```

## License

Released under the MIT License.