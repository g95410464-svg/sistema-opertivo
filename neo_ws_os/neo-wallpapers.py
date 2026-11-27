#!/usr/bin/env python3
"""
neo-wallpapers: Selector visual de fondos en paralelogramos inclinados (estilo @_snnx7)
con paleta de colores de acento, navegacion con raton/teclado y cambio en vivo.
"""
import gi
import cairo
import os
import glob
import json
import subprocess

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
gi.require_version("GdkPixbuf", "2.0")
from gi.repository import Gtk, Gdk, GdkPixbuf

WALL_DIR = "/usr/share/wallpapers/neo_ws"
STATE_FILE = os.path.expanduser("~/.config/neo_ws/state.json")

ACCENTS = [
    ("#D8DCE4", "Plata"),
    ("#06B6D4", "Cian"),
    ("#EF4444", "Rojo"),
    ("#3B82F6", "Azul"),
]


def load_state():
    try:
        with open(STATE_FILE, "r") as f:
            return json.load(f)
    except Exception:
        return {"wallpaper": "", "accent": "#3B82F6"}


def save_state(state):
    os.makedirs(os.path.dirname(STATE_FILE), exist_ok=True)
    with open(STATE_FILE, "w") as f:
        json.dump(state, f, indent=2)


def apply_wallpaper(path):
    st = load_state()
    st["wallpaper"] = path
    save_state(st)
    subprocess.Popen(["feh", "--bg-fill", path])
    # Avisar a neo-clock para actualizar contraste claro/oscuro
    subprocess.Popen(["pkill", "-USR1", "-f", "neo-clock.py"])


def apply_accent(hex_col):
    st = load_state()
    st["accent"] = hex_col
    save_state(st)
    # Actualizar acento de polybar si esta corriendo
    pconf = os.path.expanduser("~/.config/polybar/accent.ini")
    os.makedirs(os.path.dirname(pconf), exist_ok=True)
    with open(pconf, "w") as f:
        f.write(f"[colors]\naccent = {hex_col}\n")
    subprocess.Popen(["polybar-msg", "cmd", "restart"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


class WallpaperCarousel(Gtk.Window):
    def __init__(self):
        super().__init__(type=Gtk.WindowType.TOPLEVEL)
        self.set_title("neo-wallpapers")
        self.set_decorated(False)
        self.set_app_paintable(True)
        self.set_keep_above(True)
        self.set_skip_taskbar_hint(True)
        self.set_skip_pager_hint(True)

        screen = self.get_screen()
        visual = screen.get_rgba_visual()
        if visual:
            self.set_visual(visual)

        mon = screen.get_primary_monitor()
        geom = screen.get_monitor_geometry(mon)
        self.sw = geom.width
        self.sh = geom.height
        self.move(geom.x, geom.y)
        self.resize(self.sw, self.sh)

        self.paths = sorted(
            glob.glob(os.path.join(WALL_DIR, "*.jpg"))
            + glob.glob(os.path.join(WALL_DIR, "*.png"))
        )
        self.state = load_state()
        self.selected = 0
        for i, p in enumerate(self.paths):
            if p == self.state.get("wallpaper"):
                self.selected = i
                break
        self.hovered = self.selected

        self.surfaces = []
        for p in self.paths:
            try:
                pb = GdkPixbuf.Pixbuf.new_from_file_at_scale(p, 640, 420, True)
                surf = Gdk.cairo_surface_create_from_pixbuf(pb, 1, None)
                self.surfaces.append((surf, pb.get_width(), pb.get_height()))
            except Exception:
                self.surfaces.append((None, 1, 1))

        self.accent_rects = []
        self.card_polys = []

        self.da = Gtk.DrawingArea()
        self.da.add_events(
            Gdk.EventMask.POINTER_MOTION_MASK
            | Gdk.EventMask.BUTTON_PRESS_MASK
        )
        self.da.connect("draw", self.on_draw)
        self.da.connect("motion-notify-event", self.on_motion)
        self.da.connect("button-press-event", self.on_click)
        self.connect("key-press-event", self.on_key)
        self.add(self.da)
        self.show_all()

    def hex_to_rgb(self, h):
        h = h.lstrip("#")
        return tuple(int(h[i : i + 2], 16) / 255.0 for i in (0, 2, 4))

    def rounded_rect(self, cr, x, y, w, h, r):
        import math
        cr.new_sub_path()
        cr.arc(x + w - r, y + r, r, -math.pi / 2, 0)
        cr.arc(x + w - r, y + h - r, r, 0, math.pi / 2)
        cr.arc(x + r, y + h - r, r, math.pi / 2, math.pi)
        cr.arc(x + r, y + r, r, math.pi, 3 * math.pi / 2)
        cr.close_path()

    def on_draw(self, widget, cr):
        cr.set_operator(cairo.OPERATOR_SOURCE)
        cr.set_source_rgba(0.03, 0.04, 0.06, 0.42)
        cr.paint()
        cr.set_operator(cairo.OPERATOR_OVER)

        n = len(self.paths)
        if n == 0:
            return

        # 1) Capsula superior de seleccion de color de acento (identica al video)
        pill_w = 188
        pill_h = 38
        px = (self.sw - pill_w) / 2
        py = self.sh * 0.20 - 20
        self.rounded_rect(cr, px, py, pill_w, pill_h, 19)
        cr.set_source_rgba(0.11, 0.12, 0.15, 0.88)
        cr.fill_preserve()
        cr.set_source_rgba(1, 1, 1, 0.18)
        cr.set_line_width(1.2)
        cr.stroke()

        self.accent_rects = []
        cur_acc = self.state.get("accent", "#3B82F6").upper()
        for i, (col, _) in enumerate(ACCENTS):
            cx = px + 34 + i * 40
            cy = py + pill_h / 2
            r, g, b = self.hex_to_rgb(col)
            self.rounded_rect(cr, cx - 11, cy - 11, 22, 22, 6)
            cr.set_source_rgba(r, g, b, 1.0)
            cr.fill()
            if col.upper() == cur_acc:
                self.rounded_rect(cr, cx - 13, cy - 13, 26, 26, 7)
                cr.set_source_rgba(1, 1, 1, 0.95)
                cr.set_line_width(2.0)
                cr.stroke()
            self.accent_rects.append((cx - 14, cy - 14, 28, 28, col))

        # 2) Carrusel de paralelogramos inclinados (7 laminas, la activa mas alta al centro)
        skew = 78.0
        base_h = self.sh * 0.48
        base_y = self.sh * 0.30
        gap = 8.0
        total_w = self.sw * 0.96
        card_w = (total_w - (n - 1) * gap) / n
        start_x = (self.sw - total_w) / 2 + skew * 0.45

        self.card_polys = []
        order = [i for i in range(n) if i != self.hovered] + [self.hovered]

        for i in order:
            is_hi = i == self.hovered
            extra = 28.0 if is_hi else 0.0
            extra_w = 24.0 if is_hi else 0.0
            x0 = start_x + i * (card_w + gap) - extra_w / 2
            y0 = base_y - extra
            w = card_w + extra_w
            h = base_h + extra * 2

            pts = [
                (x0, y0),
                (x0 + w, y0),
                (x0 + w - skew, y0 + h),
                (x0 - skew, y0 + h),
            ]
            self.card_polys.append((i, pts))

            # Sombra profunda para la tarjeta destacada
            if is_hi:
                cr.save()
                cr.move_to(pts[0][0] + 4, pts[0][1] + 10)
                for p in pts[1:]:
                    cr.line_to(p[0] + 4, p[1] + 10)
                cr.close_path()
                cr.set_source_rgba(0, 0, 0, 0.55)
                cr.fill()
                cr.restore()

            cr.save()
            cr.move_to(*pts[0])
            for p in pts[1:]:
                cr.line_to(*p)
            cr.close_path()
            cr.clip()

            surf, iw, ih = self.surfaces[i]
            if surf:
                bbox_w = w + skew
                scale = max(bbox_w / iw, h / ih)
                dx = (x0 - skew) + (bbox_w - iw * scale) / 2
                dy = y0 + (h - ih * scale) / 2
                cr.translate(dx, dy)
                cr.scale(scale, scale)
                cr.set_source_surface(surf, 0, 0)
                cr.paint()
            else:
                cr.set_source_rgb(0.15, 0.17, 0.22)
                cr.paint()

            if not is_hi:
                cr.set_source_rgba(0, 0, 0, 0.25)
                cr.paint()
            cr.restore()

            # Borde limpio del paralelogramo
            cr.move_to(*pts[0])
            for p in pts[1:]:
                cr.line_to(*p)
            cr.close_path()
            if is_hi:
                ar, ag, ab = self.hex_to_rgb(cur_acc)
                cr.set_source_rgba(ar, ag, ab, 0.95)
                cr.set_line_width(2.6)
            else:
                cr.set_source_rgba(1, 1, 1, 0.22)
                cr.set_line_width(1.2)
            cr.stroke()

    def point_in_poly(self, x, y, poly):
        inside = False
        j = len(poly) - 1
        for i in range(len(poly)):
            xi, yi = poly[i]
            xj, yj = poly[j]
            if ((yi > y) != (yj > y)) and (x < (xj - xi) * (y - yi) / ((yj - yi) or 1e-6) + xi):
                inside = not inside
            j = i
        return inside

    def on_motion(self, widget, ev):
        for i, poly in reversed(self.card_polys):
            if self.point_in_poly(ev.x, ev.y, poly):
                if self.hovered != i:
                    self.hovered = i
                    self.da.queue_draw()
                return

    def on_click(self, widget, ev):
        for rx, ry, rw, rh, col in self.accent_rects:
            if rx <= ev.x <= rx + rw and ry <= ev.y <= ry + rh:
                self.state["accent"] = col
                apply_accent(col)
                self.da.queue_draw()
                return

        for i, poly in reversed(self.card_polys):
            if self.point_in_poly(ev.x, ev.y, poly):
                apply_wallpaper(self.paths[i])
                Gtk.main_quit()
                return

        Gtk.main_quit()

    def on_key(self, widget, ev):
        kv = ev.keyval
        if kv == Gdk.KEY_Escape:
            Gtk.main_quit()
        elif kv in (Gdk.KEY_Left, Gdk.KEY_a):
            self.hovered = (self.hovered - 1) % max(1, len(self.paths))
            self.da.queue_draw()
        elif kv in (Gdk.KEY_Right, Gdk.KEY_d):
            self.hovered = (self.hovered + 1) % max(1, len(self.paths))
            self.da.queue_draw()
        elif kv in (Gdk.KEY_Return, Gdk.KEY_KP_Enter):
            if self.paths:
                apply_wallpaper(self.paths[self.hovered])
            Gtk.main_quit()


if __name__ == "__main__":
    WallpaperCarousel()
    Gtk.main()
