# SLATE 🪐

Slate is a premium, full-screen TUI (Terminal User Interface) Arch Linux installer built with **Ratatui**. It streamlines the process of setting up a clean, optimized Arch Linux system with **Btrfs** and **systemd-boot**.

![TUI Demo](https://img.shields.io/badge/TUI-Premium-blueviolet)
![Rust](https://img.shields.io/badge/Language-Rust-orange)
![License](https://img.shields.io/badge/License-MIT-green)

## Features

- **Premium TUI Experience**: Interactive disk selection and multi-step configuration forms.
- **Automatic Partitioning**: 1GB EFI + remaining Btrfs partition with zero manual typing.
- **Optimized Btrfs Layout**: Automated subvolume creation (`@`, `@home`, `@pkg`) for easy snapshots.
- **Real-time Installation Feedback**: Live command output streaming and progress tracking directly in the TUI.
- **Bootloader Setup**: Automated `systemd-boot` installation with UUID-based persistent mounting.
- **Built-in `ax` Integration**: Automatically installs the latest version of the `ax` power tool.

## Prerequisites

- **Arch Linux ISO** (or any environment with `arch-chroot`, `pacstrap`, `sgdisk`, and `mkfs.btrfs`).
- **Network Connectivity** (required for `pacstrap`).
- **UEFI Mode** (Slate requires UEFI for `systemd-boot`).

## Quick Start

1. Boot into the Arch Linux Live ISO.
2. Download the Slate binary:
   ```bash
   curl -L https://github.com/manpreet113/slate/releases/download/tui-installer-latest/slate -o slate
   chmod +x slate
   ```
3. Run the installer:
   ```bash
   ./slate install
   ```
4. Follow the TUI prompts to select your disk and set up your user.
5. Once complete, reboot into your new system!

## Installation Stages

- **Preflight**: Automated system verification (Root, UEFI, Network).
- **Cleansing**: Wipes the target disk and creates the partition table.
- **Dance**: Sets up Btrfs subvolumes and mounts the hierarchy.
- **Injection**: Pacstraps the base system and installs essential firmware + `ax`.
- **Chroot Stage**: Finalizes the system with hostname, locale, and user setup.

## Development

Slate is written in Rust. To build from source:

```bash
cargo build --release
```

## License

Slate is released under the MIT License.