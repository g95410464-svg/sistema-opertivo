#!/usr/bin/env python3
"""
neo-clock: Reloj de escritorio futurista minimalista estilo Anurati (identico al video @_snnx7).
Se coloca detras de las ventanas, no intercepta clics y adapta su color al brillo del fondo actual.
"""
import gi
import cairo
import os
import json
import signal
import datetime

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
gi.require_version("GdkPixbuf", "2.0")
from gi.repository import Gtk, Gdk, GdkPixbuf, GLib

STATE_FILE = os.path.expanduser("~/.config/neo_ws/state.json")

# Trazos geometricos estilo Anurati (coordenadas normalizadas 0..1)
ANURATI_GLYPHS = {
    "A": [[(0.08, 1.0), (0.50, 0.05), (0.92, 1.0)]],
    "D": [[(0.15, 0.05), (0.55, 0.05), (0.85, 0.30), (0.85, 0.75), (0.55, 1.0), (0.15, 1.0)]],
    "E": [[(0.20, 0.08), (0.85, 0.08)], [(0.20, 0.52), (0.76, 0.52)], [(0.20, 0.95), (0.85, 0.95)]],
    "F": [[(0.20, 0.08), (0.85, 0.08)], [(0.20, 0.52), (0.74, 0.52)], [(0.20, 0.52), (0.20, 0.98)]],
    "H": [[(0.18, 0.05), (0.18, 1.0)], [(0.82, 0.05), (0.82, 1.0)], [(0.28, 0.52), (0.72, 0.52)]],
    "I": [[(0.50, 0.05), (0.50, 1.0)]],
    "M": [[(0.08, 1.0), (0.08, 0.08), (0.50, 0.62), (0.92, 0.08), (0.92, 1.0)]],
    "N": [[(0.15, 1.0), (0.15, 0.08), (0.85, 0.95), (0.85, 0.05)]],
    "O": [[(0.50, 0.05), (0.85, 0.25), (0.85, 0.78), (0.50, 1.0), (0.15, 0.78), (0.15, 0.25), (0.50, 0.05)]],
    "R": [[(0.18, 1.0), (0.18, 0.08), (0.68, 0.08), (0.82, 0.32), (0.64, 0.52), (0.28, 0.52)], [(0.54, 0.55), (0.86, 1.0)]],
    "S": [[(0.84, 0.14), (0.34, 0.06), (0.16, 0.26), (0.32, 0.48), (0.76, 0.58), (0.86, 0.80), (0.64, 0.98), (0.16, 0.92)]],
    "T": [[(0.10, 0.08), (0.90, 0.08)], [(0.50, 0.22), (0.50, 1.0)]],
    "U": [[(0.16, 0.05), (0.16, 0.76), (0.50, 1.0), (0.84, 0.76), (0.84, 0.05)]],
    "W": [[(0.04, 0.06), (0.28, 0.96), (0.50, 0.36), (0.72, 0.96), (0.96, 0.06)]],
    "Y": [[(0.10, 0.06), (0.50, 0.54), (0.90, 0.06)], [(0.50, 0.54), (0.50, 1.0)]],
}


def wallpaper_is_bright():
    try:
        with open(STATE_FILE, "r") as f:
            p = json.load(f).get("wallpaper", "")
        if not p or not os.path.exists(p):
            return True
        pb = GdkPixbuf.Pixbuf.new_from_file_at_scale(p, 64, 36, False)
        pix = pb.get_pixels()
        ch = pb.get_n_channels()
        rs = pb.get_rowstride()
        # Muestrear franja central (donde va el reloj)
        total = 0
        cnt = 0
        for y in range(10, 22):
            for x in range(18, 46):
                off = y * rs + x * ch
                lum = (pix[off] * 299 + pix[off + 1] * 587 + pix[off + 2] * 114) // 1000
                total += lum
                cnt += 1
        return (total / max(1, cnt)) > 118
    except Exception:
        return True


class NeoClock(Gtk.Window):
    def __init__(self):
        super().__init__(type=Gtk.WindowType.TOPLEVEL)
        self.set_title("neo-clock")
        self.set_type_hint(Gdk.WindowTypeHint.DESKTOP)
        self.set_decorated(False)
        self.set_app_paintable(True)
        self.set_keep_below(True)
        self.set_skip_taskbar_hint(True)
        self.set_skip_pager_hint(True)
        self.set_accept_focus(False)
        self.stick()

        screen = self.get_screen()
        visual = screen.get_rgba_visual()
        if visual:
            self.set_visual(visual)

        mon = screen.get_primary_monitor()
        geom = screen.get_monitor_geometry(mon)
        self.w = 780
        self.h = 190
        self.move(geom.x + (geom.width - self.w) // 2, geom.y + int(geom.height * 0.21))
        self.resize(self.w, self.h)

        self.dark_text = wallpaper_is_bright()
        self.da = Gtk.DrawingArea()
        self.da.connect("draw", self.on_draw)
        self.add(self.da)
        screen.connect("size-changed", lambda sc: self.reposition(sc))
        self.show_all()

    def reposition(self, sc):
        g = sc.get_monitor_geometry(sc.get_primary_monitor())
        self.move(g.x + (g.width - self.w) // 2, g.y + int(g.height * 0.21))

        # Click-through completo para no estorbar clics en el escritorio
        win = self.get_window()
        if win:
            win.input_shape_combine_region(cairo.Region(), 0, 0)

        GLib.timeout_add_seconds(5, self.on_tick)
        GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGUSR1, self.on_reload)

    def on_tick(self):
        self.da.queue_draw()
        return True

    def on_reload(self):
        self.dark_text = wallpaper_is_bright()
        self.da.queue_draw()
        return True

    def on_draw(self, widget, cr):
        cr.set_operator(cairo.OPERATOR_SOURCE)
        cr.set_source_rgba(0, 0, 0, 0)
        cr.paint()
        cr.set_operator(cairo.OPERATOR_OVER)

        now = datetime.datetime.now()
        day_name = now.strftime("%A").upper()
        date_str = now.strftime("%d %b %Y").upper()
        time_str = now.strftime("- %H:%M -")

        if self.dark_text:
            cr.set_source_rgba(0.08, 0.09, 0.11, 0.90)
        else:
            cr.set_source_rgba(0.96, 0.97, 0.99, 0.92)

        # Dibujar nombre del dia con geometria futurista estilo Anurati
        gw = 36.0
        gh = 44.0
        sp = 22.0
        total_w = len(day_name) * gw + (len(day_name) - 1) * sp
        ox = (self.w - total_w) / 2
        oy = 26.0

        cr.set_line_width(4.8)
        cr.set_line_cap(cairo.LINE_CAP_SQUARE)
        cr.set_line_join(cairo.LINE_JOIN_MITER)
        for idx, ch in enumerate(day_name):
            gx = ox + idx * (gw + sp)
            for stroke in ANURATI_GLYPHS.get(ch, []):
                cr.move_to(gx + stroke[0][0] * gw, oy + stroke[0][1] * gh)
                for pt in stroke[1:]:
                    cr.line_to(gx + pt[0] * gw, oy + pt[1] * gh)
                cr.stroke()

        # Fecha y hora centradas abajo
        cr.select_font_face("JetBrains Mono", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
        if self.dark_text:
            cr.set_source_rgba(0.14, 0.16, 0.20, 0.85)
        else:
            cr.set_source_rgba(0.86, 0.89, 0.94, 0.88)

        cr.set_font_size(14.5)
        ext = cr.text_extents(date_str)
        cr.move_to((self.w - ext.width) / 2, oy + gh + 36)
        cr.show_text(date_str)

        cr.set_font_size(13.5)
        ext2 = cr.text_extents(time_str)
        cr.move_to((self.w - ext2.width) / 2, oy + gh + 58)
        cr.show_text(time_str)


if __name__ == "__main__":
    NeoClock()
    Gtk.main()
