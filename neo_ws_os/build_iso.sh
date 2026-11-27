#!/usr/bin/env bash
# Generador de ISO booteable real (`neo_ws-os-x86_64.iso`) usando archiso oficial de Arch Linux.
# Puedes grabarla en un USB con Rufus o Ventoy para instalar neo_ws OS en cualquier PC real.
# Ejecutar dentro de neo_ws OS (o cualquier Arch Linux):
#   sudo bash build_iso.sh
set -euo pipefail

if [ "$EUID" -ne 0 ]; then
  echo "Ejecuta este script con sudo: sudo bash build_iso.sh"
  exit 1
fi

pacman -Sy --needed --noconfirm archiso

WORK="/tmp/neo_ws_archiso"
PROFILE="$WORK/profile"
OUT="$PWD/out_iso"

rm -rf "$WORK"
mkdir -p "$WORK" "$OUT"
cp -r /usr/share/archiso/configs/releng "$PROFILE"

# Personalizar nombre, marca e identidad de la ISO
sed -i 's/^iso_name=.*/iso_name="neo_ws_os"/' "$PROFILE/profiledef.sh"
sed -i 's/^iso_label=.*/iso_label="NEO_WS_OS"/' "$PROFILE/profiledef.sh"
sed -i 's/^iso_publisher=.*/iso_publisher="Guillermo <neo_ws OS>"/' "$PROFILE/profiledef.sh"
sed -i 's/^iso_application=.*/iso_application="neo_ws OS Live & Installer"/' "$PROFILE/profiledef.sh"

# Agregar todo el entorno grafico y apps reales de neo_ws OS a la ISO Live
cat >> "$PROFILE/packages.x86_64" <<'EOF'
xorg-server
xorg-xinit
xorg-xrandr
mesa
openbox
picom
lightdm
lightdm-gtk-greeter
polybar
rofi
feh
dunst
gtk3
python-gobject
python-cairo
papirus-icon-theme
adw-gtk-theme
gnome-themes-extra
ttf-jetbrains-mono-nerd
inter-font
noto-fonts
pipewire
pipewire-pulse
wireplumber
networkmanager
firefox
code
thunar
kitty
zsh
zsh-autosuggestions
zsh-syntax-highlighting
fastfetch
btop
cmatrix
neovim
fzf
bat
eza
tree
git
archinstall
EOF

# Copiar configuraciones y wallpapers de neo_ws OS dentro de la imagen Live
mkdir -p "$PROFILE/airootfs/usr/share/wallpapers/neo_ws" \
         "$PROFILE/airootfs/usr/local/bin" \
         "$PROFILE/airootfs/etc/skel"

cp -rf /usr/share/wallpapers/neo_ws/* "$PROFILE/airootfs/usr/share/wallpapers/neo_ws/" 2>/dev/null || true
cp -f /usr/local/bin/neo /usr/local/bin/neo-wallpapers.py /usr/local/bin/neo-clock.py /usr/local/bin/neo-dock.py /usr/local/bin/neo-demo /usr/local/bin/pipes.sh "$PROFILE/airootfs/usr/local/bin/" 2>/dev/null || true
cp -rf /home/guillermo/.config /home/guillermo/.local /home/guillermo/.zshrc "$PROFILE/airootfs/etc/skel/" 2>/dev/null || true

# Copiar branding /etc/os-release
cp -f /etc/os-release "$PROFILE/airootfs/etc/os-release"

echo "Construyendo ISO booteable real de neo_ws OS en $OUT ..."
mkarchiso -v -w "$WORK/work" -o "$OUT" "$PROFILE"
echo "ISO creada con exito en: $OUT"
ls -lh "$OUT"/*.iso
