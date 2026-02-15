#!/usr/bin/env bash
set -e

echo "[Slate] Installing packages..."
sudo pacman -Syu --noconfirm
sudo pacman -S --needed - < packages/pacman.txt

echo "[Slate] Linking dotfiles..."
mkdir -p ~/.config

ln -sf ~/Slate/dotfiles/hypr ~/.config/hypr
ln -sf ~/Slate/dotfiles/waybar ~/.config/waybar
ln -sf ~/Slate/dotfiles/wlogout ~/.config/wlogout
ln -sf ~/Slate/dotfiles/mako ~/.config/mako
ln -sf ~/Slate/dotfiles/zsh/.zshrc ~/.zshrc
ln -sf ~/Slate/dotfiles/zsh/.zprofile ~/.zprofile

echo "[Slate] Installing system configs..."
sudo cp system/limine.conf /boot/
sudo cp system/mkinitcpio.conf /etc/
sudo cp -r system/plymouth/mono-steel /usr/share/plymouth/themes/

sudo mkinitcpio -P

echo "[Slate] Done."