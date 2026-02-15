# Slate

A reproducible, dark-monochrome Arch Linux and Hyprland configuration. 

This is not a flexible, one-size-fits-all setup script. It is an opinionated, keyboard-first environment that enforces specific architectural choices, most notably full-disk encryption.

## The Stack

* **OS:** Arch Linux
* **Compositor:** Hyprland
* **Terminal:** Ghostty
* **Launcher:** Rofi (Custom Wayland configuration)
* **Bar & Notifications:** Waybar & Mako
* **Shell:** Zsh
* **Bootloader:** Limine (or systemd-boot)
* **Boot Splash:** Plymouth (Custom `mono-steel` theme)

## The Hard Requirement: LUKS Encryption

**Do not run this script on an unencrypted drive.** Slate's installation script is designed to dynamically trace virtual mapped devices (`/dev/mapper/root`) back to their physical hardware parents to inject the correct `PARTUUID` into your bootloader. 

If you install Arch Linux (via `archinstall` or manually) and do *not* set up a LUKS encrypted root partition, `install.sh` will violently reject your machine and exit. It will not attempt to guess your boot configuration.

## Installation

1. Install Arch Linux. Ensure you enable LUKS encryption.
2. Clone this repository:
   ```bash
   git clone [https://github.com/manpreet113/slate.git](https://github.com/manpreet113/slate.git) ~/Slate
   cd ~/Slate
3. Read the install.sh script. Never blindly execute a shell script that asks for sudo and touches your boot partition.
4. Run the bootstrap:
    ```bash
    chmod +x install.sh
    ./install.sh

## What the script actually does:
* Installs official packages via pacman.
* Bootstraps yay-bin and installs AUR packages.
* Idempotently symlinks the dotfiles/ directory to ~/.config/.
* Changes your default login shell to zsh.
* Installs the custom mono-steel Plymouth theme.
* Discovers your hardware PARTUUID and patches your limine.conf or systemd-boot entries.
* Rebuilds mkinitcpio to ensure keyboard input is available before the encryption hook asks for your password.

## Essential Keybindings

Slate relies heavily on Vim-style navigation and SUPER (the Windows/Command key) as the main modifier.
* `SUPER + Return`: Launch Ghostty
* `SUPER + Space`: Launch Rofi
* `SUPER + B`: Launch Zen Browser
* `SUPER + E`: Launch Thunar
* `SUPER + Q`: Kill active window
* `SUPER + CTRL + Q`: Open Wlogout menu
* `SUPER + H/J/K/L`: Move focus
* `SUPER + SHIFT + H/J/K/L`: Move window
* `SUPER + ALT + H/J/K/L`: Resize active window
* `SUPER + V`: Open clipboard manager (clipse)

## Troubleshooting

**If you end up in a tty loop after login, it is likely because your hardware requires a specific Wayland rendering flag (common in Virtual Machines). You can drop into tty2 (CTRL+ALT+F2) and investigate ~/.zprofile or ~/.zshrc.**