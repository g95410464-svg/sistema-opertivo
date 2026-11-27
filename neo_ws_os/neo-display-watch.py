#!/usr/bin/env python3
"""
neo-display-watch: Adapta automaticamente fondo (feh), Polybar (ancho exacto) y bordes
cada vez que cambia el tamano de la ventana de QEMU o se conecta otro monitor.
"""
import gi
import json
import os
import subprocess

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk, GLib

STATE_FILE = os.path.expanduser("~/.config/neo_ws/state.json")
pending_id = None


def apply_layout():
    global pending_id
    pending_id = None
    try:
        wall = json.load(open(STATE_FILE)).get(
            "wallpaper", "/usr/share/wallpapers/neo_ws/04_silk_waves.jpg"
        )
    except Exception:
        wall = "/usr/share/wallpapers/neo_ws/04_silk_waves.jpg"

    # Re-escalar fondo al 100% de la nueva resolucion (cero bordes negros!)
    subprocess.Popen(["feh", "--bg-fill", wall])
    # Reiniciar Polybar para que las 3 capsulas se adapten exactas de borde a borde
    subprocess.Popen(
        ["bash", "-c", "pkill -x polybar 2>/dev/null; sleep 0.15; polybar -c ~/.config/polybar/config.ini neo >/dev/null 2>&1 &"]
    )
    return False


def on_size_changed(screen):
    global pending_id
    if pending_id is not None:
        GLib.source_remove(pending_id)
    pending_id = GLib.timeout_add(300, apply_layout)


if __name__ == "__main__":
    screen = Gdk.Screen.get_default()
    if screen:
        screen.connect("size-changed", on_size_changed)
        screen.connect("monitors-changed", on_size_changed)
    Gtk.main()
