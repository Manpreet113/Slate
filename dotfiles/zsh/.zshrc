# Start Hyprland on tty1 login
if [[ -z $DISPLAY && $XDG_VTNR -eq 1 ]]; then
    exec start-hyprland >/dev/null 2>&1
fi