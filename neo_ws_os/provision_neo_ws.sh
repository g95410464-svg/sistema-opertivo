#!/usr/bin/env bash
set -euo pipefail

echo "=================================================="
echo "  Aprovisionando neo_ws OS (Arch Linux Real)"
echo "=================================================="

# 1. Rebranding del sistema operativo como neo_ws OS
sudo tee /etc/os-release >/dev/null <<'EOF'
NAME="neo_ws OS"
PRETTY_NAME="neo_ws OS 2026 (Arch Linux)"
ID=neo_ws
ID_LIKE=arch
BUILD_ID=rolling
ANSI_COLOR="38;2;59;130;246"
HOME_URL="https://github.com/g95410464-svg/sistema-opertivo"
LOGO=archlinux-logo
EOF

echo "neo-ws" | sudo tee /etc/hostname >/dev/null

# 2. Actualizar repositorios y llaves de pacman + instalar entorno completo real
sudo sed -i 's/^#ParallelDownloads/ParallelDownloads/' /etc/pacman.conf
sudo sed -i 's/^#Color/Color/' /etc/pacman.conf
sudo pacman -Sy --noconfirm archlinux-keyring
sudo pacman -Syu --noconfirm

PKGS=(
  # Servidor grafico, gestor de ventanas, compositor y login
  xorg-server xorg-xinit xorg-xrandr xorg-xsetroot mesa
  openbox picom lightdm lightdm-gtk-greeter
  polybar rofi feh dunst xdotool wmctrl scrot imagemagick xclip
  # GTK, iconos reales y tipografias nitidas
  gtk3 python-gobject python-cairo
  papirus-icon-theme adw-gtk-theme gnome-themes-extra
  ttf-jetbrains-mono-nerd inter-font noto-fonts noto-fonts-emoji
  # Audio HD (PipeWire) y red real
  pipewire pipewire-pulse pipewire-alsa wireplumber pavucontrol
  networkmanager network-manager-applet
  # Apps reales de uso diario + terminal + dev (exactas al video!)
  firefox code thunar thunar-archive-plugin file-roller
  kitty zsh zsh-autosuggestions zsh-syntax-highlighting
  fastfetch btop cmatrix neovim fzf bat eza tree ripgrep fd
  git curl wget unzip htop jq
)

# Filtrar paquetes disponibles para evitar fallos si alguno cambio de nombre en Arch
VALID_PKGS=()
for p in "${PKGS[@]}"; do
  if pacman -Si "$p" >/dev/null 2>&1; then
    VALID_PKGS+=("$p")
  else
    echo "[aviso] Paquete opcional omitido: $p"
  fi
done
sudo pacman -S --needed --noconfirm "${VALID_PKGS[@]}"
sudo pacman -S --needed --noconfirm cava || true

# Rebranding de GRUB para que arranque como neo_ws OS
if [ -f /etc/default/grub ]; then
  sudo sed -i 's/^GRUB_DISTRIBUTOR=.*/GRUB_DISTRIBUTOR="neo_ws OS"/' /etc/default/grub
  sudo sed -i 's/^GRUB_TIMEOUT=.*/GRUB_TIMEOUT=2/' /etc/default/grub
  sudo grub-mkconfig -o /boot/grub/grub.cfg || true
fi

# Cambiar shell de guillermo a zsh
sudo chsh -s /usr/bin/zsh guillermo || true

# 3. Instalar helper amigable `neo` (sobre pacman real) y `pipes.sh`
sudo tee /usr/local/bin/neo >/dev/null <<'EOF'
#!/usr/bin/env bash
cmd="${1:-help}"
shift || true
case "$cmd" in
  install|i)   exec sudo pacman -S "$@" ;;
  remove|rm)   exec sudo pacman -Rns "$@" ;;
  update|up)   exec sudo pacman -Syu "$@" ;;
  search|s)    exec pacman -Ss "$@" ;;
  wallpapers|w) exec /usr/local/bin/neo-wallpapers.py ;;
  demo)        exec /usr/local/bin/neo-demo ;;
  info)        exec fastfetch ;;
  *)
    echo "neo_ws OS — Gestor rapido sobre pacman real:"
    echo "  neo install <paquete>   (equivale a: sudo pacman -S <paquete>)"
    echo "  neo remove <paquete>    (equivale a: sudo pacman -Rns <paquete>)"
    echo "  neo update              (equivale a: sudo pacman -Syu)"
    echo "  neo search <texto>      (equivale a: pacman -Ss <texto>)"
    echo "  neo wallpapers          (abre el carrusel inclinado de fondos)"
    echo "  neo demo                (abre el mosaico fastfetch + btop + cmatrix + pipes)"
    ;;
esac
EOF
sudo chmod +x /usr/local/bin/neo

# pipes.sh (animacion clasica de tuberias del video!)
sudo tee /usr/local/bin/pipes.sh >/dev/null <<'EOF'
#!/usr/bin/env bash
# Animacion ligera de tuberias de colores estilo pipes.sh
tput civis; trap 'tput cnorm; tput sgr0; clear; exit 0' INT TERM
clear
w=$(tput cols); h=$(tput lines)
x=$((w/2)); y=$((h/2)); d=0; c=36
chars=("━" "┃" "┏" "┓" "┗" "┛")
while true; do
  nd=$d
  if (( RANDOM % 5 == 0 )); then
    if (( d < 2 )); then nd=$((2 + RANDOM % 2)); else nd=$((RANDOM % 2)); fi
  fi
  ch="━"
  case "$d$nd" in
    00|11) ch="━" ;; 22|33) ch="┃" ;;
    02|31) ch="┓" ;; 03|21) ch="┛" ;;
    12|30) ch="┏" ;; 13|20) ch="┗" ;;
  esac
  printf "\033[%d;%dH\033[1;%dm%s" "$y" "$x" "$c" "$ch"
  d=$nd
  case $d in 0) ((x++));; 1) ((x--));; 2) ((y++));; 3) ((y--));; esac
  if (( x <= 1 || x >= w || y <= 1 || y >= h )); then
    x=$((2 + RANDOM % (w - 3))); y=$((2 + RANDOM % (h - 3)))
    cols=(31 32 33 34 35 36); c=${cols[$((RANDOM % 6))]}
  fi
  sleep 0.025
done
EOF
sudo chmod +x /usr/local/bin/pipes.sh

# Visualizador de barras estilo cava si el paquete cava no esta instalado
if ! command -v cava >/dev/null 2>&1; then
sudo tee /usr/local/bin/cava >/dev/null <<'EOF'
#!/usr/bin/env python3
import os, sys, time, math, shutil, signal
signal.signal(signal.SIGINT, lambda *_: (print("\033[?25h\033[0m"), sys.exit(0)))
print("\033[?25l\033[2J", end="")
t = 0.0
while True:
    cols, rows = shutil.get_terminal_size((60, 18))
    bars = max(8, cols // 3)
    out = ["\033[H"]
    heights = []
    for i in range(bars):
        v = 0.5 + 0.35 * math.sin(t * 2.2 + i * 0.45) + 0.15 * math.cos(t * 3.7 - i * 0.8)
        heights.append(max(1, min(rows - 1, int(v * (rows - 1)))))
    for r in range(rows - 1, 0, -1):
        line = []
        for h in heights:
            line.append("\033[38;5;250m██ " if h >= r else "   ")
        out.append("".join(line)[:cols])
    print("\n".join(out), end="", flush=True)
    t += 0.14
    time.sleep(0.05)
EOF
sudo chmod +x /usr/local/bin/cava
fi

# 4. Instalar wallpapers HD y apps personalizadas de neo_ws OS
sudo mkdir -p /usr/share/wallpapers/neo_ws /usr/share/backgrounds/neo_ws
sudo cp -f /tmp/neo_stage/wallpapers/* /usr/share/wallpapers/neo_ws/
sudo cp -f /tmp/neo_stage/neo-wallpapers.py /usr/local/bin/neo-wallpapers.py
sudo cp -f /tmp/neo_stage/neo-clock.py /usr/local/bin/neo-clock.py
sudo cp -f /tmp/neo_stage/neo-dock.py /usr/local/bin/neo-dock.py
sudo chmod +x /usr/local/bin/neo-wallpapers.py /usr/local/bin/neo-clock.py /usr/local/bin/neo-dock.py

# Crear version suavemente desenfocada de Silk Waves para la pantalla de Login (LightDM)
magick /usr/share/wallpapers/neo_ws/04_silk_waves.jpg -blur 0x16 -modulate 75,100,100 /usr/share/backgrounds/neo_ws/login_blur.jpg \
  || convert /usr/share/wallpapers/neo_ws/04_silk_waves.jpg -blur 0x16 -modulate 75,100,100 /usr/share/backgrounds/neo_ws/login_blur.jpg \
  || sudo cp /usr/share/wallpapers/neo_ws/04_silk_waves.jpg /usr/share/backgrounds/neo_ws/login_blur.jpg

# 5. Configurar LightDM (pantalla de Login elegante y minimalista)
sudo tee /etc/lightdm/lightdm-gtk-greeter.conf >/dev/null <<'EOF'
[greeter]
background=/usr/share/backgrounds/neo_ws/login_blur.jpg
theme-name=adw-gtk3-dark
icon-theme-name=Papirus-Dark
font-name=Inter 10
xft-antialias=true
xft-rgba=rgb
indicators=~host;~spacer;~clock;~spacer;~power
clock-format=%d %b  %H:%M
position=50%,center 52%,center
default-user-image=#avatar-default
hide-user-image=false
round-user-image=true
EOF

sudo sed -i 's/^#greeter-session=.*/greeter-session=lightdm-gtk-greeter/' /etc/lightdm/lightdm.conf
sudo sed -i 's/^#user-session=.*/user-session=openbox/' /etc/lightdm/lightdm.conf
sudo tee /usr/local/bin/neo-display-setup >/dev/null <<'EOF'
#!/usr/bin/env bash
xrandr --output Virtual-1 --mode 1280x720 2>/dev/null || xrandr -s 1280x720 2>/dev/null || true
EOF
sudo chmod +x /usr/local/bin/neo-display-setup
sudo sed -i 's|^#display-setup-script=.*|display-setup-script=/usr/local/bin/neo-display-setup|' /etc/lightdm/lightdm.conf
sudo systemctl enable lightdm
sudo systemctl enable NetworkManager

# 6. Tema oscuro minimalista Openbox (NeoGlass-Dark)
THEME_DIR="/usr/share/themes/NeoGlass-Dark/openbox-3"
sudo mkdir -p "$THEME_DIR"
sudo tee "$THEME_DIR/themerc" >/dev/null <<'EOF'
border.width: 1
padding.width: 8
padding.height: 5
window.handle.width: 0
window.client.padding.width: 0
window.client.padding.height: 0
menu.overlap: 0

window.active.title.bg: flat solid
window.active.title.bg.color: #16181d
window.active.label.bg: parentrelative
window.active.label.text.color: #e2e8f0
window.active.label.text.font: shadow=n
window.active.button.unpressed.bg: parentrelative
window.active.button.unpressed.image.color: #94a3b8
window.active.button.hover.bg: parentrelative
window.active.button.hover.image.color: #ffffff
window.active.button.pressed.bg: parentrelative
window.active.button.pressed.image.color: #3b82f6
window.active.border.color: #2c313c

window.inactive.title.bg: flat solid
window.inactive.title.bg.color: #121418
window.inactive.label.bg: parentrelative
window.inactive.label.text.color: #64748b
window.inactive.button.unpressed.bg: parentrelative
window.inactive.button.unpressed.image.color: #475569
window.inactive.border.color: #1f232b

menu.border.width: 1
menu.border.color: #2e3440
menu.items.bg: flat solid
menu.items.bg.color: #16181d
menu.items.text.color: #e2e8f0
menu.items.active.bg: flat solid
menu.items.active.bg.color: #3b82f6
menu.items.active.text.color: #ffffff
menu.title.bg: flat solid
menu.title.bg.color: #101216
menu.title.text.color: #94a3b8
EOF

# 7. Configuracion del usuario guillermo (Openbox, Picom, Polybar, Plank, Kitty, Zsh, Fastfetch, Rofi, Neovim, Firefox)
H="/home/guillermo"
mkdir -p "$H/.config/openbox" "$H/.config/picom" "$H/.config/polybar" "$H/.config/kitty" \
         "$H/.config/rofi" "$H/.config/fastfetch" "$H/.config/nvim" "$H/.config/neo_ws" \
         "$H/.config/plank/dock1/launchers" "$H/.local/share/plank/themes/NeoGlass" \
         "$H/.local/share/applications" "$H/.config/gtk-3.0" "$H/tree-demo/alpha/one" \
         "$H/tree-demo/alpha/two" "$H/tree-demo/beta" "$H/tree-demo/gamma/blue" "$H/tree-demo/gamma/red"

# Estado inicial del wallpaper y acento
cat > "$H/.config/neo_ws/state.json" <<'EOF'
{
  "wallpaper": "/usr/share/wallpapers/neo_ws/04_silk_waves.jpg",
  "accent": "#3B82F6"
}
EOF

# GTK3 oscuro con iconos Papirus-Dark
cat > "$H/.config/gtk-3.0/settings.ini" <<'EOF'
[Settings]
gtk-theme-name=adw-gtk3-dark
gtk-icon-theme-name=Papirus-Dark
gtk-font-name=Inter 10
gtk-application-prefer-dark-theme=1
EOF

# Openbox rc.xml (4 escritorios, tema NeoGlass-Dark, atajos Super+W/Return/Space/tiling)
cat > "$H/.config/openbox/rc.xml" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<openbox_config xmlns="http://openbox.org/3.4/rc">
  <resistance><strength>10</strength><screen_edge_strength>20</screen_edge_strength></resistance>
  <focus><focusNew>yes</focusNew><followMouse>no</followMouse></focus>
  <placement><policy>Smart</policy><center>yes</center></placement>
  <theme>
    <name>NeoGlass-Dark</name>
    <titleLayout>LIMC</titleLayout>
    <keepBorder>yes</keepBorder>
    <animateIconify>yes</animateIconify>
    <font place="ActiveWindow"><name>Inter</name><size>9</size><weight>Bold</weight></font>
    <font place="InactiveWindow"><name>Inter</name><size>9</size></font>
    <font place="MenuHeader"><name>Inter</name><size>9</size><weight>Bold</weight></font>
    <font place="MenuItem"><name>Inter</name><size>9</size></font>
  </theme>
  <desktops>
    <number>4</number>
    <firstdesk>1</firstdesk>
    <names><name>1</name><name>2</name><name>3</name><name>4</name></names>
  </desktops>
  <margins><top>44</top><bottom>68</bottom><left>12</left><right>12</right></margins>
  <keyboard>
    <keybind key="W-Return"><action name="Execute"><command>kitty</command></action></keybind>
    <keybind key="W-w"><action name="Execute"><command>/usr/local/bin/neo-wallpapers.py</command></action></keybind>
    <keybind key="W-space"><action name="Execute"><command>rofi -show drun</command></action></keybind>
    <keybind key="W-q"><action name="Close"/></keybind>
    <keybind key="A-F4"><action name="Close"/></keybind>
    <keybind key="W-1"><action name="GoToDesktop"><to>1</to></action></keybind>
    <keybind key="W-2"><action name="GoToDesktop"><to>2</to></action></keybind>
    <keybind key="W-3"><action name="GoToDesktop"><to>3</to></action></keybind>
    <keybind key="W-4"><action name="GoToDesktop"><to>4</to></action></keybind>
    <keybind key="W-Left">
      <action name="UnmaximizeFull"/>
      <action name="MoveResizeTo"><x>0</x><y>0</y><width>50%</width><height>100%</height></action>
    </keybind>
    <keybind key="W-Right">
      <action name="UnmaximizeFull"/>
      <action name="MoveResizeTo"><x>-0</x><y>0</y><width>50%</width><height>100%</height></action>
    </keybind>
  </keyboard>
  <mouse>
    <context name="Frame">
      <mousebind button="A-Left" action="Drag"><action name="Move"/></mousebind>
      <mousebind button="A-Right" action="Drag"><action name="Resize"/></mousebind>
    </context>
    <context name="Titlebar">
      <mousebind button="Left" action="Drag"><action name="Move"/></mousebind>
      <mousebind button="Left" action="DoubleClick"><action name="ToggleMaximize"/></mousebind>
    </context>
    <context name="Close"><mousebind button="Left" action="Click"><action name="Close"/></mousebind></context>
    <context name="Maximize"><mousebind button="Left" action="Click"><action name="ToggleMaximize"/></mousebind></context>
    <context name="Iconify"><mousebind button="Left" action="Click"><action name="Iconify"/></mousebind></context>
    <context name="Client">
      <mousebind button="Left" action="Press"><action name="Focus"/><action name="Raise"/></mousebind>
    </context>
  </mouse>
</openbox_config>
EOF

# Picom (esquinas redondeadas suaves, sombras, transparencia y fade)
cat > "$H/.config/picom/picom.conf" <<'EOF'
backend = "xrender";
vsync = false;
corner-radius = 12;
rounded-corners-exclude = [
  "window_type = 'desktop'",
  "class_g = 'Polybar'",
  "class_g = 'Plank'"
];
shadow = true;
shadow-radius = 16;
shadow-offset-x = -12;
shadow-offset-y = -10;
shadow-opacity = 0.48;
shadow-exclude = [
  "window_type = 'desktop'",
  "class_g = 'Polybar'",
  "class_g = 'Plank'",
  "name = 'neo-clock'"
];
fading = true;
fade-in-step = 0.06;
fade-out-step = 0.06;
fade-delta = 6;
opacity-rule = [
  "92:class_g = 'kitty'"
];
EOF

# Polybar: 3 capsulas flotantes translúcidas (identicas a frame_01..15!)
cat > "$H/.config/polybar/accent.ini" <<'EOF'
[colors]
accent = #3B82F6
EOF

cat > "$H/.config/polybar/config.ini" <<'EOF'
include-file = ~/.config/polybar/accent.ini

[colors]
bg = #CC1A1D23
fg = #F1F5F9
fg-dim = #94A3B8
green = #4ADE80
cyan = #38BDF8

[bar/neo]
width = 98.4%
height = 30
offset-x = 0.8%
offset-y = 8
background = #00000000
foreground = ${colors.fg}
font-0 = "Inter:style=Medium:size=9.5;2"
font-1 = "JetBrainsMono Nerd Font:size=10;2"
modules-left = pill-left
modules-center = pill-center
modules-right = pill-right
enable-ipc = true
wm-restack = generic
override-redirect = false

[module/pill-left]
type = custom/script
exec = ~/.config/polybar/left_pill.sh
interval = 0.5
format = <label>
format-background = ${colors.bg}
format-padding = 2

[module/pill-center]
type = internal/date
interval = 1
date = %d %b  %H:%M:%S
format = <label>
format-background = ${colors.bg}
format-padding = 3
label = %date%

[module/pill-right]
type = custom/script
exec = ~/.config/polybar/right_pill.sh
interval = 2
format = <label>
format-background = ${colors.bg}
format-padding = 2
EOF

cat > "$H/.config/polybar/left_pill.sh" <<'EOF'
#!/usr/bin/env bash
cur=$(xprop -root _NET_CURRENT_DESKTOP 2>/dev/null | awk '{print $3}')
cur=${cur:-0}
out=""
for i in 0 1 2 3; do
  if [ "$i" = "$cur" ]; then
    out+="%{A1:wmctrl -s $i:}%{F#3B82F6}━━%{F-}%{A} "
  else
    out+="%{A1:wmctrl -s $i:}%{F#64748B}●%{F-}%{A} "
  fi
done
out+=" %{F#475569}|%{F-}  %{A1:/usr/local/bin/neo-wallpapers.py:}󰸉 Fondos%{A}  %{F#475569}|%{F-}  %{A1:/usr/local/bin/neo-demo:}󰨇 Demo%{A}"
echo "$out"
EOF

cat > "$H/.config/polybar/right_pill.sh" <<'EOF'
#!/usr/bin/env bash
ram=$(free -m | awk '/Mem:/ {printf "%.2f GB", $3/1024}')
cpu=$(grep 'cpu ' /proc/stat | awk '{usage=($2+$4)*100/($2+$4+$5)} END {printf "%d%%", usage}')
echo "%{F#4ADE80}●%{F-}  RAM ${ram}   CPU ${cpu}   %{F#475569}|%{F-}  %{A1:rofi -show drun:}󰀻 Apps%{A}   %{A1:pavucontrol:}󰕾%{A}"
EOF
chmod +x "$H/.config/polybar/left_pill.sh" "$H/.config/polybar/right_pill.sh"

# Plank tema NeoGlass (Dock flotante redondeado de cristal oscuro!)
cat > "$H/.local/share/plank/themes/NeoGlass/dock.theme" <<'EOF'
[PlankTheme]
TopRoundness=14
BottomRoundness=14
LineWidth=1
OuterStrokeColor=255;;255;;255;;38
FillStartColor=22;;25;;31;;205
FillEndColor=18;;20;;26;;215
InnerStrokeColor=255;;255;;255;;18

[PlankDockTheme]
HorizPadding=6
TopPadding=4
BottomPadding=4
ItemPadding=6
IndicatorSize=4
IconShadowSize=2
UrgentBounceHeight=1.4
LaunchBounceHeight=0.4
FadeOpacity=1
ClickTime=250
UrgentBounceTime=500
LaunchBounceTime=450
ActiveTime=250
SlideTime=250
FadeTime=200
HideTime=200
GlowSize=24
GlowTime=8000
GlowPulseTime=1500
GlowOpacity=0.8
ActiveIndicatorColor=59;;130;;246;;255
EOF

# Accesos directos personalizados para el Dock
cat > "$H/.local/share/applications/neo-btop.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=btop Monitor
Exec=kitty --title "btop — Monitor" -e btop
Icon=utilities-system-monitor
Categories=System;
EOF

cat > "$H/.local/share/applications/neo-wallpapers.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Fondos neo_ws
Exec=/usr/local/bin/neo-wallpapers.py
Icon=preferences-desktop-wallpaper
Categories=Settings;
EOF

cat > "$H/.local/share/applications/neo-demo.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Showcase neo_ws
Exec=/usr/local/bin/neo-demo
Icon=applications-science
Categories=Utility;
EOF

cat > "$H/.local/share/applications/neo-apps.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Aplicaciones
Exec=rofi -show drun
Icon=view-app-grid-symbolic
Categories=Utility;
EOF

# Anclar los 8 iconos exactos en el Dock (Plank)
cat > "$H/.config/plank/dock1/launchers/01_kitty.dockitem" <<'EOF'
[PlankDockItemPreferences]
Launcher=file:///usr/share/applications/kitty.desktop
EOF
cat > "$H/.config/plank/dock1/launchers/02_code.dockitem" <<'EOF'
[PlankDockItemPreferences]
Launcher=file:///usr/share/applications/code-oss.desktop
EOF
cat > "$H/.config/plank/dock1/launchers/03_thunar.dockitem" <<'EOF'
[PlankDockItemPreferences]
Launcher=file:///usr/share/applications/thunar.desktop
EOF
cat > "$H/.config/plank/dock1/launchers/04_firefox.dockitem" <<'EOF'
[PlankDockItemPreferences]
Launcher=file:///usr/share/applications/firefox.desktop
EOF
cat > "$H/.config/plank/dock1/launchers/05_btop.dockitem" <<'EOF'
[PlankDockItemPreferences]
Launcher=file:///home/guillermo/.local/share/applications/neo-btop.desktop
EOF
cat > "$H/.config/plank/dock1/launchers/06_wallpapers.dockitem" <<'EOF'
[PlankDockItemPreferences]
Launcher=file:///home/guillermo/.local/share/applications/neo-wallpapers.desktop
EOF
cat > "$H/.config/plank/dock1/launchers/07_demo.dockitem" <<'EOF'
[PlankDockItemPreferences]
Launcher=file:///home/guillermo/.local/share/applications/neo-demo.desktop
EOF
cat > "$H/.config/plank/dock1/launchers/08_apps.dockitem" <<'EOF'
[PlankDockItemPreferences]
Launcher=file:///home/guillermo/.local/share/applications/neo-apps.desktop
EOF

# Kitty config (fuente JetBrainsMono Nerd, fondo oscuro elegante, bordes suaves)
cat > "$H/.config/kitty/kitty.conf" <<'EOF'
font_family      JetBrainsMono Nerd Font
font_size        10.5
background       #14161A
foreground       #E2E8F0
background_opacity 0.92
window_padding_width 12
cursor_shape     beam
confirm_os_window_close 0
enable_audio_bell no
color0  #1E222A
color1  #F87171
color2  #4ADE80
color3  #FACC15
color4  #3B82F6
color5  #C084FC
color6  #38BDF8
color7  #E2E8F0
EOF

# Rofi tema cristal oscuro minimalista
cat > "$H/.config/rofi/config.rasi" <<'EOF'
configuration {
  modi: "drun,run";
  show-icons: true;
  icon-theme: "Papirus-Dark";
  font: "Inter 11";
  display-drun: "Buscar app";
}
* {
  bg: #15181EE6;
  bg-alt: #222732;
  fg: #F1F5F9;
  accent: #3B82F6;
  background-color: transparent;
  text-color: @fg;
}
window {
  width: 540px;
  background-color: @bg;
  border: 1px;
  border-color: #FFFFFF22;
  border-radius: 16px;
  padding: 18px;
}
inputbar {
  background-color: @bg-alt;
  border-radius: 10px;
  padding: 10px 14px;
  margin: 0 0 12px 0;
  children: [prompt, entry];
}
prompt { text-color: @accent; margin: 0 10px 0 0; }
listview { lines: 7; spacing: 4px; }
element { padding: 8px 10px; border-radius: 8px; }
element selected { background-color: @accent; text-color: #FFFFFF; }
element-icon { size: 24px; margin: 0 10px 0 0; }
EOF

# Fastfetch con branding neo_ws OS
cat > "$H/.config/fastfetch/config.jsonc" <<'EOF'
{
  "$schema": "https://github.com/fastfetch-cli/fastfetch/raw/dev/doc/json_schema.json",
  "logo": {
    "type": "small",
    "color": { "1": "blue", "2": "cyan" }
  },
  "modules": [
    "title",
    "separator",
    "os",
    "kernel",
    "uptime",
    "packages",
    "shell",
    "display",
    "wm",
    "terminal",
    "cpu",
    "gpu",
    "memory",
    "disk",
    "localip",
    "break",
    "colors"
  ]
}
EOF

# Neovim limpio
cat > "$H/.config/nvim/init.lua" <<'EOF'
vim.opt.number = true
vim.opt.relativenumber = true
vim.opt.termguicolors = true
vim.opt.tabstop = 4
vim.opt.shiftwidth = 4
vim.opt.expandtab = true
vim.opt.mouse = "a"
vim.cmd("colorscheme habamax")
EOF

# Zsh con prompt moderno + aliases fcd/fse (exactos al video!) + autosuggestions
cat > "$H/.zshrc" <<'EOF'
export PATH="$HOME/.local/bin:/usr/local/bin:$PATH"
setopt PROMPT_SUBST
PROMPT='%F{39}guillermo@neo-ws%f %F{81}%~%f %F{77}❯%f '

alias ls='eza --icons'
alias ll='eza -la --icons'
alias cat='bat --paging=never'
alias fcd='cd "$(fd -t d | fzf --preview "tree -C {} | head -30")"'
alias fse='fzf --preview "bat --style=numbers --color=always {}"'
alias update='sudo pacman -Syu'

[ -f /usr/share/zsh/plugins/zsh-autosuggestions/zsh-autosuggestions.zsh ] && source /usr/share/zsh/plugins/zsh-autosuggestions/zsh-autosuggestions.zsh
[ -f /usr/share/zsh/plugins/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh ] && source /usr/share/zsh/plugins/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh
EOF

# Carpeta ~/tree-demo igual a las capturas del video (frame_11..15)
cat > "$H/tree-demo/example.py" <<'EOF'
from pathlib import Path

def load_items(directory: Path) -> list[str]:
    return sorted(
        item.name
        for item in directory.iterdir()
        if item.is_file()
    )

def main() -> None:
    directory = Path(".")
    items = load_items(directory)
    for item in items:
        print(f"- {item}")

if __name__ == "__main__":
    main()
EOF
cat > "$H/tree-demo/alpha/one/data.json" <<'EOF'
{
  "os": "neo_ws OS",
  "base": "Arch Linux",
  "wallpaper_count": 7,
  "border_color": "#3B82F6"
}
EOF
echo "Hola desde neo_ws OS" > "$H/tree-demo/notes.md"
echo "sample" > "$H/tree-demo/gamma/red/sample.txt"

# Script Showcase `neo-demo`: abre el mosaico del video (fastfetch + cmatrix + cava / btop)!
sudo tee /usr/local/bin/neo-demo >/dev/null <<'EOF'
#!/usr/bin/env bash
wmctrl -s 1 2>/dev/null || true
kitty --title "guillermo@neo-ws — fastfetch" -e zsh -c "fastfetch; exec zsh" &
P1=$!
kitty --title "cmatrix — cmatrix -C white -a -b -u 3" -e cmatrix -C white -a -b -u 3 &
P2=$!
kitty --title "btop — Monitor" -e btop &
P3=$!
sleep 1.4
wmctrl -r "fastfetch" -e 0,20,52,612,584 2>/dev/null || true
wmctrl -r "cmatrix"   -e 0,646,52,614,284 2>/dev/null || true
wmctrl -r "btop"      -e 0,646,350,614,286 2>/dev/null || true
EOF
sudo chmod +x /usr/local/bin/neo-demo

# Openbox autostart: resolucion 1280x720, fondo HD, picom, polybar, neo-dock y reloj Anurati!
cat > "$H/.config/openbox/autostart" <<'EOF'
xrandr --output Virtual-1 --mode 1280x720 2>/dev/null || xrandr -s 1280x720 2>/dev/null || true
WALL=$(python3 -c 'import json,os; print(json.load(open(os.path.expanduser("~/.config/neo_ws/state.json")))["wallpaper"])' 2>/dev/null || echo "/usr/share/wallpapers/neo_ws/04_silk_waves.jpg")
feh --bg-fill "$WALL" &
picom --config ~/.config/picom/picom.conf &
dunst &
polybar -c ~/.config/polybar/config.ini neo &
(sleep 0.8 && /usr/local/bin/neo-dock.py) &
(sleep 1.0 && /usr/local/bin/neo-clock.py) &
EOF
chmod +x "$H/.config/openbox/autostart"

# Asegurar permisos del usuario guillermo
sudo chown -R guillermo:guillermo "$H"

# 8. Politicas limpias de Firefox (marcadores Google, YouTube, GitHub, sin pantallas molestas)
sudo mkdir -p /usr/lib/firefox/distribution
sudo tee /usr/lib/firefox/distribution/policies.json >/dev/null <<'EOF'
{
  "policies": {
    "DisableTelemetry": true,
    "DontCheckDefaultBrowser": true,
    "OverrideFirstRunPage": "",
    "OverridePostUpdatePage": "",
    "Homepage": {
      "URL": "https://www.google.com",
      "StartPage": "homepage"
    },
    "Bookmarks": [
      { "Title": "Google", "URL": "https://www.google.com", "Placement": "toolbar" },
      { "Title": "YouTube", "URL": "https://www.youtube.com", "Placement": "toolbar" },
      { "Title": "GitHub", "URL": "https://github.com/g95410464-svg/sistema-opertivo", "Placement": "toolbar" },
      { "Title": "Discord", "URL": "https://discord.com/app", "Placement": "toolbar" },
      { "Title": "Arch Wiki", "URL": "https://wiki.archlinux.org", "Placement": "toolbar" }
    ]
  }
}
EOF

# Activar audio PipeWire para el usuario
sudo -u guillermo XDG_RUNTIME_DIR=/run/user/$(id -u guillermo) systemctl --user enable pipewire pipewire-pulse wireplumber 2>/dev/null || true

# Deshabilitar cloud-init en arranques posteriores para boot ultra-rapido (~5 seg!)
sudo touch /etc/cloud/cloud-init.disabled
sync

echo "=================================================="
echo "  neo_ws OS APROVISIONADO AL 100% CON EXITO!"
echo "=================================================="
