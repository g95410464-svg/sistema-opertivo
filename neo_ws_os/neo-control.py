#!/usr/bin/env python3
"""
neo-control: Centro de Control desplegable estilo GNOME 49 / macOS para neo_ws OS.
Incluye sliders reales de Volumen (PipeWire) y Brillo (xrandr), toggles rapidos y controles de energia.
"""
import gi
import cairo
import json
import math
import os
import subprocess

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk

PID_FILE = "/tmp/neo_control.pid"
STATE_FILE = os.path.expanduser("~/.config/neo_ws/state.json")


def toggle_existing():
    if os.path.exists(PID_FILE):
        try:
            old_pid = int(open(PID_FILE).read().strip())
            os.kill(old_pid, 15)
            os.remove(PID_FILE)
            return True
        except Exception:
            try:
                os.remove(PID_FILE)
            except Exception:
                pass
    return False


def read_accent():
    try:
        h = json.load(open(STATE_FILE)).get("accent", "#3B82F6").lstrip("#")
        return tuple(int(h[i : i + 2], 16) / 255.0 for i in (0, 2, 4))
    except Exception:
        return (0.23, 0.51, 0.96)


def get_primary_output():
    try:
        out = subprocess.check_output(["xrandr", "--query"], text=True, stderr=subprocess.DEVNULL)
        for line in out.splitlines():
            if " connected" in line:
                return line.split()[0]
    except Exception:
        pass
    return "Virtual-1"


def get_brightness():
    try:
        out = subprocess.check_output(["xrandr", "--verbose"], text=True, stderr=subprocess.DEVNULL)
        for line in out.splitlines():
            if "Brightness:" in line:
                v = float(line.split(":")[1].strip())
                return max(30, min(100, int(round(v * 100))))
    except Exception:
        pass
    return 100


def set_brightness(pct):
    out = get_primary_output()
    val = max(0.30, min(1.0, pct / 100.0))
    subprocess.Popen(["xrandr", "--output", out, "--brightness", f"{val:.2f}"])


def get_volume():
    try:
        out = subprocess.check_output(
            ["wpctl", "get-volume", "@DEFAULT_AUDIO_SINK@"], text=True, stderr=subprocess.DEVNULL
        )
        muted = "MUTED" in out
        val = float(out.split()[1])
        return max(0, min(100, int(round(val * 100)))), muted
    except Exception:
        return 75, False


def set_volume(pct):
    v = max(0.0, min(1.0, pct / 100.0))
    subprocess.Popen(["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "0"])
    subprocess.Popen(["wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", f"{v:.2f}"])


def toggle_mute():
    subprocess.run(["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"], stderr=subprocess.DEVNULL)


def get_local_ip():
    try:
        out = subprocess.check_output(["hostname", "-I"], text=True, stderr=subprocess.DEVNULL).strip()
        return out.split()[0] if out else "Desconectado"
    except Exception:
        return "10.0.2.15"


class NeoControl(Gtk.Window):
    def __init__(self):
        super().__init__(type=Gtk.WindowType.TOPLEVEL)
        self.set_title("neo-control")
        self.set_decorated(False)
        self.set_app_paintable(True)
        self.set_keep_above(True)
        self.set_skip_taskbar_hint(True)
        self.set_skip_pager_hint(True)

        screen = self.get_screen()
        visual = screen.get_rgba_visual()
        if visual:
            self.set_visual(visual)

        self.w = 336
        self.h = 344
        g = screen.get_monitor_geometry(screen.get_primary_monitor())
        self.move(g.x + g.width - self.w - 10, g.y + 42)
        self.resize(self.w, self.h)

        self.accent = read_accent()
        self.vol, self.muted = get_volume()
        self.bright = get_brightness()
        self.ip = get_local_ip()
        self.dragging = None
        self.buttons = []

        self.da = Gtk.DrawingArea()
        self.da.add_events(
            Gdk.EventMask.BUTTON_PRESS_MASK
            | Gdk.EventMask.BUTTON_RELEASE_MASK
            | Gdk.EventMask.POINTER_MOTION_MASK
        )
        self.da.connect("draw", self.on_draw)
        self.da.connect("button-press-event", self.on_press)
        self.da.connect("button-release-event", self.on_release)
        self.da.connect("motion-notify-event", self.on_motion)
        self.connect("key-press-event", self.on_key)
        self.connect("focus-out-event", lambda *_: self.quit_clean())
        self.add(self.da)
        self.show_all()

    def quit_clean(self):
        try:
            if os.path.exists(PID_FILE):
                os.remove(PID_FILE)
        except Exception:
            pass
        Gtk.main_quit()

    def rounded_rect(self, cr, x, y, w, h, r):
        cr.new_sub_path()
        cr.arc(x + w - r, y + r, r, -math.pi / 2, 0)
        cr.arc(x + w - r, y + h - r, r, 0, math.pi / 2)
        cr.arc(x + r, y + h - r, r, math.pi / 2, math.pi)
        cr.arc(x + r, y + r, r, math.pi, 3 * math.pi / 2)
        cr.close_path()

    def draw_slider(self, cr, y, label, pct, muted=False):
        sx = 22.0
        sw = self.w - 44.0
        ar, ag, ab = self.accent

        cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
        cr.set_font_size(11.5)
        cr.set_source_rgba(0.84, 0.88, 0.94, 0.95)
        cr.move_to(sx, y)
        cr.show_text(label)

        val_txt = "Silencio" if muted else f"{pct}%"
        ext = cr.text_extents(val_txt)
        cr.set_source_rgba(0.95, 0.97, 1.0, 0.95)
        cr.move_to(sx + sw - ext.width, y)
        cr.show_text(val_txt)

        ty = y + 12.0
        th = 9.0
        self.rounded_rect(cr, sx, ty, sw, th, 4.5)
        cr.set_source_rgba(1, 1, 1, 0.14)
        cr.fill()

        fill_w = max(12.0, sw * (0 if muted else pct) / 100.0)
        self.rounded_rect(cr, sx, ty, fill_w, th, 4.5)
        cr.set_source_rgba(ar, ag, ab, 0.95)
        cr.fill()

        knob_x = max(sx + 7, min(sx + sw - 7, sx + sw * (0 if muted else pct) / 100.0))
        cr.arc(knob_x, ty + th / 2, 7.5, 0, 2 * math.pi)
        cr.set_source_rgba(1, 1, 1, 1)
        cr.fill()

    def on_draw(self, widget, cr):
        cr.set_operator(cairo.OPERATOR_SOURCE)
        cr.set_source_rgba(0, 0, 0, 0)
        cr.paint()
        cr.set_operator(cairo.OPERATOR_OVER)

        self.buttons = []
        ar, ag, ab = self.accent

        # Tarjeta principal de cristal oscuro redondeada
        self.rounded_rect(cr, 2, 2, self.w - 4, self.h - 4, 18)
        cr.set_source_rgba(0.08, 0.09, 0.12, 0.92)
        cr.fill_preserve()
        cr.set_source_rgba(1, 1, 1, 0.20)
        cr.set_line_width(1.1)
        cr.stroke()

        # Encabezado
        cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
        cr.set_font_size(13.0)
        cr.set_source_rgba(0.96, 0.97, 1.0, 1.0)
        cr.move_to(22, 32)
        cr.show_text("Centro de Control")

        cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
        cr.set_font_size(10.5)
        cr.set_source_rgba(0.58, 0.64, 0.72, 0.95)
        cr.move_to(self.w - 86, 31)
        cr.show_text("neo_ws OS")

        # 1) Slider Volumen real
        self.draw_slider(cr, 62, "Volumen", self.vol, self.muted)
        # 2) Slider Brillo real
        self.draw_slider(cr, 114, "Brillo de pantalla", self.bright, False)

        # 3) 4 Tarjetas rapidas 2x2 (estilo GNOME 49!)
        tiles = [
            ("net", "Red Ethernet", self.ip, True),
            ("mute", "Sonido", "Silenciado" if self.muted else f"Activo ({self.vol}%)", not self.muted),
            ("wall", "Fondos HD", "Carrusel 7 temas", False),
            ("mixer", "Mezclador", "Abrir pavucontrol", False),
        ]
        tw = 140.0
        th = 54.0
        for idx, (tid, title, sub, active) in enumerate(tiles):
            col = idx % 2
            row = idx // 2
            tx = 22.0 + col * (tw + 12.0)
            ty = 156.0 + row * (th + 12.0)
            self.rounded_rect(cr, tx, ty, tw, th, 13)
            if active:
                cr.set_source_rgba(ar, ag, ab, 0.88)
            else:
                cr.set_source_rgba(1, 1, 1, 0.09)
            cr.fill_preserve()
            cr.set_source_rgba(1, 1, 1, 0.14)
            cr.set_line_width(1.0)
            cr.stroke()

            cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
            cr.set_font_size(11.5)
            cr.set_source_rgba(1, 1, 1, 0.98)
            cr.move_to(tx + 12, ty + 23)
            cr.show_text(title)

            cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
            cr.set_font_size(10.0)
            cr.set_source_rgba(0.88, 0.92, 0.97, 0.85 if active else 0.72)
            cr.move_to(tx + 12, ty + 41)
            cr.show_text(sub)
            self.buttons.append((tx, ty, tw, th, tid))

        # 4) Fila inferior de energia: Bloquear | Reiniciar | Apagar
        py = 290.0
        pw = 91.0
        ph = 34.0
        pwr = [
            ("lock", "Bloquear", (1, 1, 1, 0.10)),
            ("reboot", "Reiniciar", (1, 1, 1, 0.10)),
            ("poweroff", "Apagar", (0.90, 0.28, 0.30, 0.78)),
        ]
        for idx, (pid, lbl, rgba) in enumerate(pwr):
            px = 22.0 + idx * (pw + 9.5)
            self.rounded_rect(cr, px, py, pw, ph, 11)
            cr.set_source_rgba(*rgba)
            cr.fill()
            cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
            cr.set_font_size(11.0)
            cr.set_source_rgba(1, 1, 1, 0.96)
            ext = cr.text_extents(lbl)
            cr.move_to(px + (pw - ext.width) / 2 - ext.x_bearing, py + 21)
            cr.show_text(lbl)
            self.buttons.append((px, py, pw, ph, pid))

    def update_slider_from_x(self, kind, x):
        sx = 22.0
        sw = self.w - 44.0
        ratio = max(0.0, min(1.0, (x - sx) / sw))
        if kind == "vol":
            self.vol = int(round(ratio * 100))
            self.muted = self.vol == 0
            set_volume(self.vol)
        elif kind == "bright":
            self.bright = int(round(30 + ratio * 70))
            set_brightness(self.bright)
        self.da.queue_draw()

    def on_press(self, widget, ev):
        if 64 <= ev.y <= 92 and 18 <= ev.x <= self.w - 18:
            self.dragging = "vol"
            self.update_slider_from_x("vol", ev.x)
            return
        if 116 <= ev.y <= 144 and 18 <= ev.x <= self.w - 18:
            self.dragging = "bright"
            self.update_slider_from_x("bright", ev.x)
            return
        for bx, by, bw, bh, bid in self.buttons:
            if bx <= ev.x <= bx + bw and by <= ev.y <= by + bh:
                if bid == "mute":
                    toggle_mute()
                    self.vol, self.muted = get_volume()
                    self.da.queue_draw()
                elif bid == "wall":
                    subprocess.Popen(["/usr/local/bin/neo-wallpapers.py"], start_new_session=True)
                    self.quit_clean()
                elif bid == "mixer":
                    subprocess.Popen(["pavucontrol"], start_new_session=True)
                    self.quit_clean()
                elif bid == "net":
                    subprocess.Popen(["nm-connection-editor"], start_new_session=True)
                    self.quit_clean()
                elif bid == "lock":
                    subprocess.Popen(["dm-tool", "lock"])
                    self.quit_clean()
                elif bid == "reboot":
                    subprocess.Popen(["systemctl", "reboot"])
                    self.quit_clean()
                elif bid == "poweroff":
                    subprocess.Popen(["systemctl", "poweroff"])
                    self.quit_clean()
                return

    def on_motion(self, widget, ev):
        if self.dragging:
            self.update_slider_from_x(self.dragging, ev.x)

    def on_release(self, widget, ev):
        self.dragging = None

    def on_key(self, widget, ev):
        if ev.keyval == Gdk.KEY_Escape:
            self.quit_clean()


if __name__ == "__main__":
    if toggle_existing():
        raise SystemExit(0)
    with open(PID_FILE, "w") as f:
        f.write(str(os.getpid()))
    NeoControl()
    Gtk.main()
