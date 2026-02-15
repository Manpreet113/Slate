#!/usr/bin/env bash

# Fail fast on errors, unbound variables, and hidden pipe failures
set -euo pipefail

# Dynamically discover the repository root regardless of folder name or execution path
REPO_DIR="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)"
echo "[Slate] Bootstrapping from: $REPO_DIR"

PACMAN_LIST="$REPO_DIR/packages/pacman.txt"
AUR_LIST="$REPO_DIR/packages/aur.txt"
DOTFILES_DIR="$REPO_DIR/dotfiles"
SYSTEM_DIR="$REPO_DIR/system"

echo "[Slate] Synchronizing repositories and updating system..."
sudo pacman -Syu --noconfirm

echo "[Slate] Installing official packages..."
grep -vE '^\s*#|^\s*$' "$PACMAN_LIST" | sudo pacman -Syu --needed --noconfirm -

# Evaluate the AUR helper situation and bootstrap yay-bin if needed
if ! command -v yay >/dev/null 2>&1; then
    echo "[Slate] No AUR helper found. Bootstrapping yay-bin..."
    TMP_DIR=$(mktemp -d)
    git clone https://aur.archlinux.org/yay-bin.git "$TMP_DIR/yay-bin"
    (cd "$TMP_DIR/yay-bin" && makepkg -si --noconfirm)
    rm -rf "$TMP_DIR"
fi

echo "[Slate] Installing AUR packages..."
grep -vE '^\s*#|^\s*$' "$AUR_LIST" | yay -Sy --needed --noconfirm -

echo "[Slate] Linking dotfiles..."
mkdir -p ~/.config

# Use -snf to prevent directory nesting if the script is run twice
ln -snf "$DOTFILES_DIR/hypr" ~/.config/hypr
ln -snf "$DOTFILES_DIR/waybar" ~/.config/waybar
ln -snf "$DOTFILES_DIR/wlogout" ~/.config/wlogout
ln -snf "$DOTFILES_DIR/mako" ~/.config/mako
ln -snf "$DOTFILES_DIR/ghostty" ~/.config/ghostty
ln -snf "$DOTFILES_DIR/rofi" ~/.config/rofi
ln -snf "$DOTFILES_DIR/zsh/.zshrc" ~/.zshrc
ln -snf "$DOTFILES_DIR/zsh/.zprofile" ~/.zprofile

echo "[Slate] Verifying default shell..."
if [ "${SHELL}" != "/usr/bin/zsh" ]; then
    echo "[Slate] Changing default shell to zsh. You may be prompted for your password."
    chsh -s /usr/bin/zsh
else
    echo "[Slate] zsh is already the default shell."
fi

echo "[Slate] Installing system configs..."
sudo cp "$SYSTEM_DIR/mkinitcpio.conf" /etc/mkinitcpio.conf

# Install Plymouth theme safely
sudo mkdir -p /usr/share/plymouth/themes/mono-steel
sudo cp -a "$SYSTEM_DIR/mono-steel/." /usr/share/plymouth/themes/mono-steel/

echo "[Slate] Discovering root partition UUID..."
ROOT_DEVICE=$(findmnt / -no SOURCE)

# The LUKS / Device Mapper Intercept
if [[ "$ROOT_DEVICE" == /dev/mapper/* ]]; then
    echo "[Slate] Virtual mapped device detected. Tracing to physical parent..."
    
    DM_NAME=$(basename "$ROOT_DEVICE")

    # Extract the parent kernel name (e.g., vda2)
    PARENT_NAME=$(lsblk -nro NAME,PKNAME | awk -v dev="$DM_NAME" '$1 == dev {print $2}')
    
    if [ -z "$PARENT_NAME" ]; then
        echo "[Error] Could not resolve physical parent for $ROOT_DEVICE."
        exit 1
    fi
    
    # Reconstruct the physical path
    ROOT_DEVICE="/dev/$PARENT_NAME"
    echo "[Slate] Physical device resolved to: $ROOT_DEVICE"
fi

# Now query the actual hardware, whether it was direct or resolved
ROOT_UUID=$(sudo blkid -s PARTUUID -o value "$ROOT_DEVICE")

if [ -z "$ROOT_UUID" ]; then
    echo "[Error] Could not determine root PARTUUID. Aborting bootloader patch to prevent bricking."
    exit 1
fi

echo "[Slate] Root PARTUUID is $ROOT_UUID."

if [ -z "$ROOT_UUID" ]; then
    echo "[Error] Could not determine root PARTUUID. Aborting bootloader patch to prevent bricking."
    exit 1
fi

echo "[Slate] Root PARTUUID is $ROOT_UUID."

# The Bootloader Switchboard
if [ -d "/boot/limine" ] || [ -f "/boot/limine.conf" ]; then
    echo "[Slate] Detected Limine. Patching limine.conf..."
    sudo mkdir -p /boot/limine
    sudo cp "$SYSTEM_DIR/limine.conf" /boot/limine/
    sudo sed -i "s/{{ROOT_PARTUUID}}/$ROOT_UUID/g" /boot/limine/limine.conf

elif [ -d "/boot/loader/entries" ]; then
    echo "[Slate] Detected systemd-boot."
    ENTRY_FILE=$(find /boot/loader/entries/ -name "*.conf" | grep -i "arch" | head -n 1)
    
    if [ -n "$ENTRY_FILE" ]; then
        echo "[Slate] Patching systemd-boot entry: $ENTRY_FILE"
        sudo sed -i -E "s/root=PARTUUID=[a-zA-Z0-9-]+/root=PARTUUID=$ROOT_UUID/g" "$ENTRY_FILE"
    else
        echo "[Warning] Found systemd-boot directory, but no Arch entry file to patch!"
    fi
else
    echo "[Warning] Unknown bootloader. You will need to configure your boot parameters manually."
fi

echo "[Slate] Setting Plymouth theme to mono-steel and rebuilding initcpio..."
# -R handles both setting the theme and running mkinitcpio
sudo plymouth-set-default-theme -R mono-steel

echo "[Slate] Done. Reboot to enter the void."