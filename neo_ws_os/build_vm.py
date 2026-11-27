#!/usr/bin/env python3
"""
Orquestador automatizado para crear la VM real `neo_ws_os.qcow2`:
1. Sirve cloud-init (nocloud-net) en 127.0.0.1:8765
2. Arranca Arch Linux en QEMU (4 GB RAM, 4 vCPUs WHPX, virtio-net SSH :2222)
3. Sube wallpapers HD, neo-wallpapers.py, neo-clock.py y provision_neo_ws.sh
4. Ejecuta provision_neo_ws.sh con pacman real y apaga limpiamente la VM
"""
import http.server
import os
import socketserver
import subprocess
import sys
import threading
import time

BASE = r"C:\Users\Administrator\neo_ws_os"
QEMU = r"C:\Program Files\qemu\qemu-system-x86_64.exe"
KEY = os.path.join(BASE, "neo_ssh_key")
SSH_OPTS = [
    "-i", KEY,
    "-p", "2222",
    "-o", "StrictHostKeyChecking=no",
    "-o", "UserKnownHostsFile=NUL",
    "-o", "ConnectTimeout=5",
    "-o", "BatchMode=yes",
]


def ensure_lf(path):
    with open(path, "rb") as f:
        data = f.read()
    clean = data.replace(b"\r\n", b"\n")
    if clean != data:
        with open(path, "wb") as f:
            f.write(clean)


def start_cidata_server():
    cidata_dir = os.path.join(BASE, "cidata")
    for name in ("meta-data", "user-data"):
        ensure_lf(os.path.join(cidata_dir, name))

    class Handler(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, directory=cidata_dir, **kwargs)

        def log_message(self, fmt, *args):
            print(f"[cidata] {fmt % args}", flush=True)

    httpd = socketserver.TCPServer(("127.0.0.1", 8765), Handler)
    t = threading.Thread(target=httpd.serve_forever, daemon=True)
    t.start()
    return httpd


def ssh_run(cmd, check=True, timeout=None):
    return subprocess.run(
        ["ssh"] + SSH_OPTS + ["guillermo@127.0.0.1", cmd],
        check=check,
        timeout=timeout,
    )


def main():
    for fn in ("provision_neo_ws.sh", "neo-wallpapers.py", "neo-clock.py", "neo-dock.py"):
        ensure_lf(os.path.join(BASE, fn))

    httpd = start_cidata_server()
    serial_log = os.path.join(BASE, "boot_serial.log")
    if os.path.exists(serial_log):
        os.remove(serial_log)

    qemu_cmd = [
        QEMU,
        "-m", "4G",
        "-smp", "4",
        "-accel", "whpx,kernel-irqchip=off",
        "-accel", "tcg,thread=multi",
        "-drive", f"file={os.path.join(BASE, 'neo_ws_os.qcow2')},if=virtio,format=qcow2",
        "-netdev", "user,id=net0,hostfwd=tcp::2222-:22",
        "-device", "virtio-net-pci,netdev=net0",
        "-device", "virtio-vga",
        "-smbios", "type=1,serial=ds=nocloud-net;s=http://10.0.2.2:8765/",
        "-display", "none",
        "-serial", f"file:{serial_log}",
    ]
    print("[builder] Arrancando Arch Linux en QEMU (4 GB RAM / 4 vCPUs WHPX)...", flush=True)
    proc = subprocess.Popen(qemu_cmd)

    try:
        # Esperar a que cloud-init termine y sshd responda
        ready = False
        for intento in range(90):
            if proc.poll() is not None:
                print(f"[builder] ERROR: QEMU salio prematuramente con codigo {proc.returncode}", flush=True)
                sys.exit(1)
            r = subprocess.run(
                ["ssh"] + SSH_OPTS + ["guillermo@127.0.0.1", "echo READY"],
                capture_output=True,
                text=True,
            )
            if r.returncode == 0 and "READY" in r.stdout:
                print(f"[builder] SSH y cloud-init listos en intento {intento + 1}!", flush=True)
                ready = True
                break
            time.sleep(2)

        if not ready:
            print("[builder] ERROR: Tiempo agotado esperando SSH / cloud-init.", flush=True)
            sys.exit(1)

        # Subir archivos de configuracion y wallpapers HD
        print("[builder] Subiendo wallpapers HD y scripts de neo_ws OS...", flush=True)
        ssh_run("mkdir -p /tmp/neo_stage/wallpapers")
        scp_base = [
            "scp", "-i", KEY, "-P", "2222",
            "-o", "StrictHostKeyChecking=no",
            "-o", "UserKnownHostsFile=NUL",
        ]
        subprocess.run(
            scp_base + [
                os.path.join(BASE, "provision_neo_ws.sh"),
                os.path.join(BASE, "neo-wallpapers.py"),
                os.path.join(BASE, "neo-clock.py"),
                os.path.join(BASE, "neo-dock.py"),
                "guillermo@127.0.0.1:/tmp/neo_stage/",
            ],
            check=True,
        )
        subprocess.run(
            scp_base + ["-r", os.path.join(BASE, "wallpapers"), "guillermo@127.0.0.1:/tmp/neo_stage/"],
            check=True,
        )

        # Ejecutar aprovisionamiento completo (pacman + escritorio + Firefox + VS Code)
        print("[builder] Instalando paquetes reales con pacman y configurando escritorio...", flush=True)
        ssh_run("bash /tmp/neo_stage/provision_neo_ws.sh", check=True, timeout=1800)

        # Apagar VM limpiamente con sync previo (critico para Btrfs!)
        print("[builder] Sincronizando disco y apagando VM limpiamente...", flush=True)
        ssh_run("sync && sudo poweroff", check=False)
        try:
            proc.wait(timeout=45)
        except subprocess.TimeoutExpired:
            proc.terminate()
        print("[builder] LISTO! neo_ws_os.qcow2 aprovisionado con exito.", flush=True)
    finally:
        httpd.shutdown()
        if proc.poll() is None:
            try:
                ssh_run("sync && sudo poweroff", check=False, timeout=15)
                proc.wait(timeout=20)
            except Exception:
                proc.terminate()


if __name__ == "__main__":
    main()
