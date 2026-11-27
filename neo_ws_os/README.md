# neo_ws OS (Distribución Linux real basada en Arch Linux)

Sistema operativo de escritorio minimalista y funcional de uso diario basado en **Arch Linux**, diseñado para correr fluido en QEMU (4 GB RAM, 4 vCPUs WHPX, mouse 0-lag `usb-tablet`, audio HD DirectSound e Internet Gigabit) y con generador de **ISO booteable real** para instalar en una PC física con USB.

## Características principales

- **Gestor de paquetes oficial `pacman`** (`sudo pacman -S firefox`, `sudo pacman -S discord`, `sudo pacman -S steam`) + comando rápido `neo install <paquete>`.
- **Internet real completo (HTTPS/TLS 1.3)** con **Firefox** preinstalado (Google, YouTube, Discord Web, GitHub).
- **Entorno de desarrollo listo:** **VS Code (`code-oss`)**, **Kitty**, **Zsh** con autosugerencias y resaltado, **Neovim**, `fastfetch`, `btop`, `cmatrix`, `cava`, `pipes.sh`, `fzf` + `bat` (`fcd` y `fse`) y `tree`.
- **Escritorio estilo minimalista cristal oscuro:**
  - Pantalla de Login **LightDM** con fondo desenfocado (`guillermo` / clave `neo`).
  - Barra superior flotante de 3 cápsulas (**Polybar**).
  - Reloj de escritorio futurista estilo *Anurati* (`neo-clock.py`) que adapta su contraste al fondo.
  - Selector de **7 fondos HD fotográficos reales** en paralelogramos inclinados (`Super + W` o `neo wallpapers`) con selector de color de acento.
  - Dock inferior flotante con iconos **Papirus-Dark** reales (**Plank**).

## Cómo arrancar en Windows (QEMU)

```powershell
cd C:\Users\Administrator\neo_ws_os
powershell -ExecutionPolicy Bypass -File .\run_neo_ws.ps1
```

- **Usuario:** `guillermo`
- **Contraseña:** `neo` (también tiene `sudo` real)
- **Atajos útiles:**
  - `Super + W`: Selector de fondos inclinado
  - `Super + Enter`: Abrir Terminal (Kitty)
  - `Super + Espacio`: Lanzador de aplicaciones (Rofi)
  - `Super + 1..4`: Cambiar de escritorio
  - `Super + Izq / Der`: Acomodar ventanas en mosaico 50/50
  - `neo demo`: Abrir el mosaico `fastfetch` + `cmatrix` + `btop`

## Cómo generar una ISO booteable para instalar en una PC real (USB)

Dentro de `neo_ws OS` (o cualquier Arch Linux), ejecuta:

```bash
sudo bash build_iso.sh
```

La imagen `neo_ws_os-*.iso` aparecerá en `out_iso/` lista para grabar en USB con Rufus o Ventoy (incluye `archinstall` para instalar en tu disco duro).
