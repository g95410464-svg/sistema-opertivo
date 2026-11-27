# Lanzador oficial de neo_ws OS (Arch Linux real en QEMU: 4 GB RAM, 4 vCPUs WHPX, mouse 0-lag, audio HD e Internet real)
$Base = Split-Path -Parent $MyInvocation.MyCommand.Path
$Qemu = "C:\Program Files\qemu\qemu-system-x86_64.exe"
$Disk = Join-Path $Base "neo_ws_os.qcow2"

if (-not (Test-Path $Disk)) {
    Write-Error "No se encontro $Disk"
    exit 1
}

Write-Host "Iniciando neo_ws OS (4 GB RAM | 4 vCPUs WHPX | Audio HD | Red Gigabit | Mouse 0-lag)..." -ForegroundColor Cyan
Write-Host "Usuario: guillermo  |  Clave: neo" -ForegroundColor Gray
Write-Host "Atajos: Super+W (Fondos) | Super+Enter (Terminal) | Super+Espacio (Apps)" -ForegroundColor DarkGray

& $Qemu `
    -name "neo_ws OS" `
    -m 4G `
    -smp 4 `
    -accel whpx,kernel-irqchip=off `
    -accel tcg,thread=multi `
    -drive "file=$Disk,if=virtio,format=qcow2" `
    -device virtio-vga,xres=1280,yres=720 `
    -usb -device usb-tablet `
    -netdev user,id=net0,hostfwd=tcp::2222-:22 `
    -device virtio-net-pci,netdev=net0 `
    -audiodev dsound,id=snd0 `
    -device intel-hda -device hda-duplex,audiodev=snd0 `
    -display gtk,zoom-to-fit=on
