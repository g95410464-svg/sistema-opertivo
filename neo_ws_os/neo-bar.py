#!/usr/bin/env python3
"""
neo-bar: Barra superior vectorial nativa en GTK3+Cairo con 3 capsulas flotantes de cristal transparente:
- Izquierda: Escritorios numerados [1] 2 3 4 + Fondos + Demo (sin costuras ni simbolos rotos)
- Centro: Fecha y hora en vivo -> clic despliega neo-calendar.py
- Derecha: RAM, CPU, Volumen % y boton de Control -> clic despliega neo-control.py
"""
import gi
import cairo
import datetime
import json
import math
import os
import subprocess

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk, GLib

STATE_FILE = os.path.expanduser("~/.config/neo_ws/state.json")
MESES = ["", "Ene", "Feb", "Mar", "Abr", "May", "Jun", "Jul", "Ago", "Sep", "Oct", "Nov", "Dic"]


def read_accent():
    try:
        h = json.load(open(STATE_FILE)).get("accent", "#3B82F6").lstrip("#")
        return tuple(int(h[i : i + 2], 16) / 255.0 for i in (0, 2, 4))
    except Exception:
        return (0.23, 0.51, 0.96)


def get_desktop():
    try:
        out = subprocess.check_output(
            ["xprop", "-root", "_NET_CURRENT_DESKTOP"], text=True, stderr=subprocess.DEVNULL
        )
        return int(out.strip().split()[-1])
    except Exception:
        return 0


def get_ram_str():
    try:
        info = {}
        with open("/proc/meminfo") as f:
            for line in f:
                k, v = line.split(":", 1)
                info[k] = int(v.strip().split()[0])
        used_kb = info["MemTotal"] - info.get("MemAvailable", info["MemFree"])
        return f"{used_kb / (1024 * 1024):.2f} GB"
    except Exception:
        return "0.5 GB"


def get_volume():
    try:
        out = subprocess.check_output(
            ["wpctl", "get-volume", "@DEFAULT_AUDIO_SINK@"], text=True, stderr=subprocess.DEVNULL
        )
        muted = "MUTED" in out
        val = float(out.split()[1])
        return int(round(val * 100)), muted
    except Exception:
        return 75, False


class NeoBar(Gtk.Window):
    def __init__(self):
        super().__init__(type=Gtk.WindowType.TOPLEVEL)
        self.set_title("neo-bar")
        self.set_type_hint(Gdk.WindowTypeHint.DOCK)
        self.set_decorated(False)
        self.set_app_paintable(True)
        self.set_keep_above(True)
        self.set_skip_taskbar_hint(True)
        self.set_skip_pager_hint(True)
        self.set_accept_focus(False)
        self.stick()

        screen = self.get_screen()
        visual = screen.get_rgba_visual()
        if visual:
            self.set_visual(visual)

        self.bar_h = 40
        self.sw = 1280
        self.cur_desk = 0
        self.ram_str = "0.55 GB"
        self.cpu_pct = 5
        self.vol_pct = 75
        self.muted = False
        self.prev_cpu = None
        self.accent = read_accent()

        self.hover_id = None
        self.hitboxes = []

        self.da = Gtk.DrawingArea()
        self.da.add_events(
            Gdk.EventMask.POINTER_MOTION_MASK
            | Gdk.EventMask.LEAVE_NOTIFY_MASK
            | Gdk.EventMask.BUTTON_PRESS_MASK
        )
        self.da.connect("draw", self.on_draw)
        self.da.connect("motion-notify-event", self.on_motion)
        self.da.connect("leave-notify-event", self.on_leave)
        self.da.connect("button-press-event", self.on_click)
        self.add(self.da)

        self.reposition()
        screen.connect("size-changed", lambda *_: self.reposition())
        screen.connect("monitors-changed", lambda *_: self.reposition())
        self.show_all()

        self.poll_fast()
        self.poll_stats()
        GLib.timeout_add(450, self.poll_fast)
        GLib.timeout_add(1600, self.poll_stats)

    def reposition(self):
        sc = self.get_screen()
        g = sc.get_monitor_geometry(sc.get_primary_monitor())
        self.sw = g.width
        self.resize(self.sw, self.bar_h)
        self.move(g.x, g.y)
        self.da.queue_draw()

    def poll_fast(self):
        self.cur_desk = get_desktop()
        self.accent = read_accent()
        self.da.queue_draw()
        return True

    def poll_stats(self):
        self.ram_str = get_ram_str()
        self.vol_pct, self.muted = get_volume()
        try:
            with open("/proc/stat") as f:
                vals = [int(x) for x in f.readline().split()[1:8]]
            idle = vals[3] + vals[4]
            total = sum(vals)
            if self.prev_cpu is not None:
                dt = max(1, total - self.prev_cpu[0])
                di = idle - self.prev_cpu[1]
                self.cpu_pct = max(0, min(100, int(100 * (dt - di) / dt)))
            self.prev_cpu = (total, idle)
        except Exception:
            pass
        self.da.queue_draw()
        return True

    def rounded_rect(self, cr, x, y, w, h, r):
        cr.new_sub_path()
        cr.arc(x + w - r, y + r, r, -math.pi / 2, 0)
        cr.arc(x + w - r, y + h - r, r, 0, math.pi / 2)
        cr.arc(x + r, y + h - r, r, math.pi / 2, math.pi)
        cr.arc(x + r, y + r, r, math.pi, 3 * math.pi / 2)
        cr.close_path()

    def draw_glass_pill(self, cr, x, y, w, h):
        r = h / 2.0
        self.rounded_rect(cr, x + 1, y + 2, w - 2, h, r)
        cr.set_source_rgba(0, 0, 0, 0.22)
        cr.fill()
        self.rounded_rect(cr, x, y, w, h, r)
        cr.set_source_rgba(0.08, 0.09, 0.12, 0.48)
        cr.fill_preserve()
        cr.set_source_rgba(1, 1, 1, 0.24)
        cr.set_line_width(1.0)
        cr.stroke()

    def draw_speaker_icon(self, cr, x, y, muted):
        cr.save()
        cr.set_source_rgba(0.95, 0.97, 1.0, 0.95)
        cr.move_to(x, y - 3)
        cr.line_to(x + 3.5, y - 3)
        cr.line_to(x + 7.5, y - 6.5)
        cr.line_to(x + 7.5, y + 6.5)
        cr.line_to(x + 3.5, y + 3)
        cr.line_to(x, y + 3)
        cr.close_path()
        cr.fill()
        cr.set_line_width(1.4)
        if muted:
            cr.set_source_rgba(0.97, 0.44, 0.44, 0.95)
            cr.move_to(x + 10, y - 3.5)
            cr.line_to(x + 15, y + 3.5)
            cr.move_to(x + 15, y - 3.5)
            cr.line_to(x + 10, y + 3.5)
            cr.stroke()
        else:
            cr.arc(x + 8.5, y, 3.6, -0.75, 0.75)
            cr.stroke()
            cr.arc(x + 8.5, y, 6.2, -0.75, 0.75)
            cr.stroke()
        cr.restore()

    def draw_sliders_icon(self, cr, x, y):
        cr.save()
        cr.set_source_rgba(0.95, 0.97, 1.0, 0.92)
        cr.set_line_width(1.4)
        cr.move_to(x, y - 3)
        cr.line_to(x + 12, y - 3)
        cr.move_to(x, y + 3)
        cr.line_to(x + 12, y + 3)
        cr.stroke()
        cr.arc(x + 8.5, y - 3, 2.2, 0, 2 * math.pi)
        cr.fill()
        cr.arc(x + 3.5, y + 3, 2.2, 0, 2 * math.pi)
        cr.fill()
        cr.restore()

    def on_draw(self, widget, cr):
        cr.set_operator(cairo.OPERATOR_SOURCE)
        cr.set_source_rgba(0, 0, 0, 0)
        cr.paint()
        cr.set_operator(cairo.OPERATOR_OVER)

        self.hitboxes = []
        py = 6.0
        ph = 30.0
        ar, ag, ab = self.accent

        # ========== 1) CAPSULA IZQUIERDA: [1] [2] [3] [4] | Fondos | Demo ==========
        lx = 10.0
        lw = 248.0
        self.draw_glass_pill(cr, lx, py, lw, ph)

        cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
        cr.set_font_size(11.5)
        for ws in range(4):
            bx = lx + 8 + ws * 28
            by = py + 4
            bw = 24
            bh = 22
            active = ws == self.cur_desk
            hov = self.hover_id == f"ws_{ws}"
            if active:
                self.rounded_rect(cr, bx, by, bw, bh, 11)
                cr.set_source_rgba(ar, ag, ab, 0.95)
                cr.fill()
                cr.set_source_rgba(1, 1, 1, 1)
            elif hov:
                self.rounded_rect(cr, bx, by, bw, bh, 11)
                cr.set_source_rgba(1, 1, 1, 0.14)
                cr.fill()
                cr.set_source_rgba(0.95, 0.97, 1, 1)
            else:
                cr.set_source_rgba(0.84, 0.88, 0.94, 0.85)
            txt = str(ws + 1)
            ext = cr.text_extents(txt)
            cr.move_to(bx + (bw - ext.width) / 2 - ext.x_bearing, py + 19.5)
            cr.show_text(txt)
            self.hitboxes.append((bx, py, bw, ph, f"ws_{ws}"))

        # Separador sutil
        cr.set_source_rgba(1, 1, 1, 0.22)
        cr.set_line_width(1.0)
        cr.move_to(lx + 123, py + 8)
        cr.line_to(lx + 123, py + ph - 8)
        cr.stroke()

        # Boton Fondos + Boton Demo
        cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
        cr.set_font_size(11.5)
        for bid, label, bx, bw in [("fondos", "Fondos", lx + 129, 58), ("demo", "Demo", lx + 190, 50)]:
            if self.hover_id == bid:
                self.rounded_rect(cr, bx, py + 4, bw, 22, 11)
                cr.set_source_rgba(1, 1, 1, 0.16)
                cr.fill()
            cr.set_source_rgba(0.96, 0.97, 0.99, 0.96)
            ext = cr.text_extents(label)
            cr.move_to(bx + (bw - ext.width) / 2 - ext.x_bearing, py + 19.5)
            cr.show_text(label)
            self.hitboxes.append((bx, py, bw, ph, bid))

        # ========== 2) CAPSULA CENTRAL: Fecha y Hora (clic -> Calendario) ==========
        now = datetime.datetime.now()
        clock_txt = f"{now.day:02d} {MESES[now.month]}  {now.strftime('%H:%M:%S')}"
        cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
        cr.set_font_size(11.5)
        cext = cr.text_extents(clock_txt)
        cw = cext.width + 36
        cx = (self.sw - cw) / 2.0
        self.draw_glass_pill(cr, cx, py, cw, ph)
        if self.hover_id == "calendar":
            self.rounded_rect(cr, cx + 2, py + 2, cw - 4, ph - 4, (ph - 4) / 2.0)
            cr.set_source_rgba(1, 1, 1, 0.10)
            cr.fill()
        cr.set_source_rgba(0.97, 0.98, 1.0, 0.98)
        cr.move_to(cx + (cw - cext.width) / 2 - cext.x_bearing, py + 19.5)
        cr.show_text(clock_txt)
        self.hitboxes.append((cx, py, cw, ph, "calendar"))

        # ========== 3) CAPSULA DERECHA: Telemetria + Sonido + Centro de Control ==========
        rw = 278.0
        rx = self.sw - rw - 10.0
        self.draw_glass_pill(cr, rx, py, rw, ph)
        if self.hover_id == "control":
            self.rounded_rect(cr, rx + 2, py + 2, rw - 4, ph - 4, (ph - 4) / 2.0)
            cr.set_source_rgba(1, 1, 1, 0.10)
            cr.fill()

        # Punto verde red activa
        cr.arc(rx + 16, py + ph / 2, 4.0, 0, 2 * math.pi)
        cr.set_source_rgba(0.29, 0.87, 0.50, 1.0)
        cr.fill()

        cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
        cr.set_font_size(11.2)
        cr.set_source_rgba(0.95, 0.97, 1.0, 0.95)
        cr.move_to(rx + 27, py + 19.5)
        cr.show_text(f"{self.ram_str}   CPU {self.cpu_pct}%")

        # Divisor antes de Sonido + Controles
        cr.set_source_rgba(1, 1, 1, 0.22)
        cr.set_line_width(1.0)
        cr.move_to(rx + 158, py + 8)
        cr.line_to(rx + 158, py + ph - 8)
        cr.stroke()

        # Icono altavoz vectorial + porcentaje volumen
        self.draw_speaker_icon(cr, rx + 168, py + ph / 2, self.muted)
        vol_txt = "Mute" if self.muted else f"{self.vol_pct}%"
        cr.set_source_rgba(0.96, 0.97, 1.0, 0.96)
        cr.move_to(rx + 188, py + 19.5)
        cr.show_text(vol_txt)

        # Icono sliders + punto acento (indica menu desplegable de control)
        self.draw_sliders_icon(cr, rx + 236, py + ph / 2)
        cr.arc(rx + 262, py + ph / 2, 4.2, 0, 2 * math.pi)
        cr.set_source_rgba(ar, ag, ab, 0.95)
        cr.fill()

        self.hitboxes.append((rx, py, rw, ph, "control"))

    def hit_test(self, x, y):
        for bx, by, bw, bh, bid in self.hitboxes:
            if bx <= x <= bx + bw and by <= y <= by + bh:
                return bid
        return None

    def on_motion(self, widget, ev):
        h = self.hit_test(ev.x, ev.y)
        if h != self.hover_id:
            self.hover_id = h
            self.da.queue_draw()

    def on_leave(self, widget, ev):
        if self.hover_id is not None:
            self.hover_id = None
            self.da.queue_draw()

    def on_click(self, widget, ev):
        bid = self.hit_test(ev.x, ev.y)
        if not bid:
            return
        if bid.startswith("ws_"):
            ws = bid.split("_")[1]
            subprocess.Popen(["wmctrl", "-s", ws])
        elif bid == "fondos":
            subprocess.Popen(["/usr/local/bin/neo-wallpapers.py"], start_new_session=True)
        elif bid == "demo":
            subprocess.Popen(["/usr/local/bin/neo-demo"], start_new_session=True)
        elif bid == "calendar":
            subprocess.Popen(["/usr/local/bin/neo-calendar.py"], start_new_session=True)
        elif bid == "control":
            subprocess.Popen(["/usr/local/bin/neo-control.py"], start_new_session=True)


if __name__ == "__main__":
    NeoBar()
    Gtk.main()
