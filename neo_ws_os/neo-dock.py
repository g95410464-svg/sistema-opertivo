#!/usr/bin/env python3
"""
neo-dock: Dock flotante estilo macOS/GNOME (identico al video @_snnx7) con cristal oscuro
redondeado, iconos reales de alta resolucion (Papirus-Dark), zoom suave al pasar el mouse y tooltips.
"""
import gi
import cairo
import math
import subprocess

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
gi.require_version("GdkPixbuf", "2.0")
from gi.repository import Gtk, Gdk, GLib

ITEMS = [
    ("Terminal", "utilities-terminal", ["kitty"]),
    ("VS Code", "visual-studio-code", ["code"]),
    ("Archivos", "system-file-manager", ["thunar"]),
    ("Firefox", "firefox", ["firefox"]),
    ("btop Monitor", "utilities-system-monitor", ["kitty", "-o", "font_size=9.5", "-o", "initial_window_width=100c", "-o", "initial_window_height=34c", "-o", "remember_window_size=no", "--title", "btop — Monitor", "-e", "btop"]),
    ("Fondos", "preferences-desktop-wallpaper", ["/usr/local/bin/neo-wallpapers.py"]),
    ("Showcase", "applications-science", ["/usr/local/bin/neo-demo"]),
    ("Aplicaciones", "view-app-grid-symbolic", ["rofi", "-show", "drun"]),
]

FALLBACK_ICONS = {
    "visual-studio-code": ["code-oss", "com.visualstudio.code.oss", "text-editor"],
    "utilities-terminal": ["kitty", "terminal"],
    "firefox": ["firefox-esr", "web-browser"],
    "system-file-manager": ["thunar", "folder"],
    "utilities-system-monitor": ["htop", "system-monitor"],
    "preferences-desktop-wallpaper": ["preferences-desktop-theme", "image-x-generic"],
    "applications-science": ["applications-development", "applications-graphics"],
    "view-app-grid-symbolic": ["start-here", "application-x-executable"],
}


class NeoDock(Gtk.Window):
    def __init__(self):
        super().__init__(type=Gtk.WindowType.TOPLEVEL)
        self.set_title("neo-dock")
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

        mon = screen.get_primary_monitor()
        geom = screen.get_monitor_geometry(mon)

        self.icon_size = 42
        self.spacing = 12
        self.pad_x = 16
        self.pill_h = 58
        self.top_space = 34
        self.pill_w = self.pad_x * 2 + len(ITEMS) * self.icon_size + (len(ITEMS) - 1) * self.spacing
        self.win_w = self.pill_w + 24
        self.win_h = self.pill_h + self.top_space + 8

        self.move(geom.x + (geom.width - self.win_w) // 2, geom.y + geom.height - self.win_h - 6)
        self.resize(self.win_w, self.win_h)

        self.theme = Gtk.IconTheme.get_default()
        self.icons = [self.load_icon_surface(ic) for _, ic, _ in ITEMS]
        self.hovered = -1

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
        self.show_all()

    def load_icon_surface(self, name):
        candidates = [name] + FALLBACK_ICONS.get(name, [])
        for cand in candidates:
            try:
                pb = self.theme.load_icon(cand, 48, Gtk.IconLookupFlags.FORCE_SIZE)
                if pb:
                    return Gdk.cairo_surface_create_from_pixbuf(pb, 1, None)
            except Exception:
                pass
        return None

    def rounded_rect(self, cr, x, y, w, h, r):
        cr.new_sub_path()
        cr.arc(x + w - r, y + r, r, -math.pi / 2, 0)
        cr.arc(x + w - r, y + h - r, r, 0, math.pi / 2)
        cr.arc(x + r, y + h - r, r, math.pi / 2, math.pi)
        cr.arc(x + r, y + r, r, math.pi, 3 * math.pi / 2)
        cr.close_path()

    def item_x(self, i):
        px = (self.win_w - self.pill_w) / 2
        return px + self.pad_x + i * (self.icon_size + self.spacing)

    def on_draw(self, widget, cr):
        cr.set_operator(cairo.OPERATOR_SOURCE)
        cr.set_source_rgba(0, 0, 0, 0)
        cr.paint()
        cr.set_operator(cairo.OPERATOR_OVER)

        px = (self.win_w - self.pill_w) / 2
        py = self.top_space

        # Sombra suave bajo el dock
        self.rounded_rect(cr, px + 2, py + 3, self.pill_w - 4, self.pill_h, 16)
        cr.set_source_rgba(0, 0, 0, 0.45)
        cr.fill()

        # Capsula de cristal oscuro redondeada
        self.rounded_rect(cr, px, py, self.pill_w, self.pill_h, 16)
        cr.set_source_rgba(0.09, 0.10, 0.13, 0.84)
        cr.fill_preserve()
        cr.set_source_rgba(1, 1, 1, 0.18)
        cr.set_line_width(1.1)
        cr.stroke()

        # Iconos Papirus-Dark reales + elevacion al pasar el raton
        for i, (label, _, _) in enumerate(ITEMS):
            ix = self.item_x(i)
            is_hov = i == self.hovered
            scale = 1.16 if is_hov else 1.0
            lift = 6.0 if is_hov else 0.0
            sz = self.icon_size * scale
            dx = ix + (self.icon_size - sz) / 2
            dy = py + (self.pill_h - sz) / 2 - lift

            surf = self.icons[i]
            if surf:
                cr.save()
                cr.translate(dx, dy)
                cr.scale(sz / 48.0, sz / 48.0)
                cr.set_source_surface(surf, 0, 0)
                cr.paint()
                cr.restore()
            else:
                self.rounded_rect(cr, dx, dy, sz, sz, 10)
                cr.set_source_rgba(0.23, 0.51, 0.96, 0.9)
                cr.fill()

            # Tooltip flotante limpio arriba del icono activo
            if is_hov:
                cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
                cr.set_font_size(11.5)
                ext = cr.text_extents(label)
                tw = ext.width + 18
                th = 22
                tx = max(4, min(self.win_w - tw - 4, ix + self.icon_size / 2 - tw / 2))
                ty = py - th - 8
                self.rounded_rect(cr, tx, ty, tw, th, 8)
                cr.set_source_rgba(0.07, 0.09, 0.12, 0.92)
                cr.fill_preserve()
                cr.set_source_rgba(1, 1, 1, 0.16)
                cr.set_line_width(1.0)
                cr.stroke()
                cr.set_source_rgba(0.96, 0.97, 0.99, 1.0)
                cr.move_to(tx + 9, ty + 15)
                cr.show_text(label)

    def hit_test(self, x, y):
        if y < self.top_space - 6 or y > self.top_space + self.pill_h + 4:
            return -1
        for i in range(len(ITEMS)):
            ix = self.item_x(i)
            if ix - 4 <= x <= ix + self.icon_size + 4:
                return i
        return -1

    def on_motion(self, widget, ev):
        h = self.hit_test(ev.x, ev.y)
        if h != self.hovered:
            self.hovered = h
            self.da.queue_draw()

    def on_leave(self, widget, ev):
        if self.hovered != -1:
            self.hovered = -1
            self.da.queue_draw()

    def on_click(self, widget, ev):
        idx = self.hit_test(ev.x, ev.y)
        if 0 <= idx < len(ITEMS):
            cmd = ITEMS[idx][2]
            subprocess.Popen(cmd, start_new_session=True)


if __name__ == "__main__":
    NeoDock()
    Gtk.main()
