#!/usr/bin/env bash

# Fail fast on errors, unbound variables, and hidden pipe failures
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "[Slate] Running from: $REPO_DIR"

PACMAN_LIST="$REPO_DIR/packages/pacman.txt"
AUR_LIST="$REPO_DIR/packages/aur.txt"
DOTFILES_DIR="$REPO_DIR/dotfiles"
SYSTEM_DIR="$REPO_DIR/system"
echo "[Slate] Synchronizing repositories and updating system..."
sudo pacman -Syu --noconfirm

echo "[Slate] Installing official packages..."
# Strip out comments and empty lines before passing to pacman
grep -vE '^\s*#|^\s*$' "$PACMAN_LIST" | sudo pacman -S --needed --noconfirm -

# Evaluate the AUR helper situation
if command -v yay >/dev/null 2>&1; then
    AUR_HELPER="yay"
elif command -v paru >/dev/null 2>&1; then
    AUR_HELPER="paru"
else
    echo "[Slate] No AUR helper found. Bootstrapping yay-bin..."
    TMP_DIR=$(mktemp -d)
    git clone https://aur.archlinux.org/yay-bin.git "$TMP_DIR/yay-bin"
    (cd "$TMP_DIR/yay-bin" && makepkg -si --noconfirm)
    rm -rf "$TMP_DIR"
    AUR_HELPER="yay"
fi

echo "[Slate] Installing AUR packages using $AUR_HELPER..."
grep -vE '^\s*#|^\s*$' "$AUR_LIST" | $AUR_HELPER -S --needed --noconfirm -

echo "[Slate] Linking dotfiles..."
mkdir -p ~/.config

# Use -snf to prevent directory nesting if the script is run twice
ln -snf "$REPO_DIR/dotfiles/hypr" ~/.config/hypr
ln -snf "$REPO_DIR/dotfiles/waybar" ~/.config/waybar
ln -snf "$REPO_DIR/dotfiles/wlogout" ~/.config/wlogout
ln -snf "$REPO_DIR/dotfiles/mako" ~/.config/mako
ln -snf "$REPO_DIR/dotfiles/ghostty" ~/.config/ghostty
ln -snf "$REPO_DIR/dotfiles/rofi" ~/.config/rofi

# Zsh configurations
ln -snf "$REPO_DIR/dotfiles/zsh/.zshrc" ~/.zshrc
ln -snf "$REPO_DIR/dotfiles/zsh/.zprofile" ~/.zprofile

echo "[Slate] Installing system configs..."

# Explicitly create destination folders before copying
sudo mkdir -p /boot/limine
sudo mkdir -p /usr/share/plymouth/themes/mono-steel

echo "[Slate] Installing system configs..."

# Explicitly create destination folders
sudo mkdir -p /boot/limine
sudo mkdir -p /usr/share/plymouth/themes/mono-steel

echo "[Slate] Discovering root partition UUID..."
# Find the physical device mounted at /
ROOT_DEVICE=$(findmnt / -no SOURCE)

# Extract just the PARTUUID value
ROOT_UUID=$(sudo blkid -s PARTUUID -o value "$ROOT_DEVICE")

# Fail instantly if we didn't find a UUID
if [ -z "$ROOT_UUID" ]; then
    echo "[Error] Could not determine root PARTUUID. Aborting to prevent unbootable system."
    exit 1
fi

echo "[Slate] Root PARTUUID is $ROOT_UUID. Patching limine.conf..."

# Copy the template to the boot directory
sudo cp "$REPO_DIR/system/limine.conf" /boot/limine/

# Use sed to swap the placeholder for the real UUID in place
sudo sed -i "s/{{ROOT_PARTUUID}}/$ROOT_UUID/g" /boot/limine/limine.conf

# Copy mkinitcpio and Plymouth configs
sudo cp "$REPO_DIR/system/mkinitcpio.conf" /etc/
sudo cp -a "$REPO_DIR/system/mono-steel/." /usr/share/plymouth/themes/mono-steel/

echo "[Slate] Setting Plymouth theme to mono-steel and rebuilding initcpio..."
sudo plymouth-set-default-theme -R mono-steel

echo "[Slate] Done. Reboot to enter the void."