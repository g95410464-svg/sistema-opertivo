#!/usr/bin/env python3
"""
neo-dock: Dock flotante estilo macOS/GNOME con AUTO-HIDE INTELIGENTE (Intellihide):
- Visible cuando el escritorio esta libre o ninguna ventana invade la zona del dock.
- Se oculta automaticamente cuando abres o maximizas una app para NO tapar nunca su contenido.
- Al acercar el raton al borde inferior de la pantalla reaparece al instante.
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
    ("btop Monitor", "utilities-system-monitor", [
        "kitty", "-o", "font_size=9.5", "-o", "initial_window_width=105c",
        "-o", "initial_window_height=34c", "-o", "remember_window_size=no",
        "--title", "btop — Monitor", "-e", "btop",
    ]),
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


def get_current_desktop():
    try:
        out = subprocess.check_output(
            ["xprop", "-root", "_NET_CURRENT_DESKTOP"], text=True, stderr=subprocess.DEVNULL
        )
        return int(out.strip().split()[-1])
    except Exception:
        return 0


def any_window_overlaps_dock(dock_y, sw, sh):
    """Devuelve True si alguna ventana normal visible en el escritorio actual invade la franja del dock."""
    try:
        cur_desk = get_current_desktop()
        out = subprocess.check_output(["wmctrl", "-lG"], text=True, stderr=subprocess.DEVNULL)
        for line in out.splitlines():
            parts = line.split(None, 7)
            if len(parts) < 8:
                continue
            wid, desk_s, x_s, y_s, w_s, h_s, _host, title = parts
            desk = int(desk_s)
            if desk != cur_desk:
                continue
            low = title.lower()
            if any(skip in low for skip in ("neo-dock", "neo-clock", "neo-calendar", "neo-wallpapers", "polybar")):
                continue
            y = int(y_s)
            h = int(h_s)
            # Verificar si la ventana esta minimizada/oculta
            st = subprocess.check_output(
                ["xprop", "-id", wid, "_NET_WM_STATE"], text=True, stderr=subprocess.DEVNULL
            )
            if "_NET_WM_STATE_HIDDEN" in st:
                continue
            if "_NET_WM_STATE_MAXIMIZED_VERT" in st or (y + h) >= (dock_y - 4):
                return True
    except Exception:
        pass
    return False


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

        self.icon_size = 42
        self.spacing = 12
        self.pad_x = 16
        self.pill_h = 58
        self.top_space = 34
        self.pill_w = self.pad_x * 2 + len(ITEMS) * self.icon_size + (len(ITEMS) - 1) * self.spacing
        self.win_w = self.pill_w + 24
        self.win_h = self.pill_h + self.top_space + 8

        self.theme = Gtk.IconTheme.get_default()
        self.icons = [self.load_icon_surface(ic) for _, ic, _ in ITEMS]
        self.hovered = -1
        self.pointer_inside = False
        self.hidden = False

        self.da = Gtk.DrawingArea()
        self.da.add_events(
            Gdk.EventMask.POINTER_MOTION_MASK
            | Gdk.EventMask.ENTER_NOTIFY_MASK
            | Gdk.EventMask.LEAVE_NOTIFY_MASK
            | Gdk.EventMask.BUTTON_PRESS_MASK
        )
        self.da.connect("draw", self.on_draw)
        self.da.connect("motion-notify-event", self.on_motion)
        self.da.connect("enter-notify-event", self.on_enter)
        self.da.connect("leave-notify-event", self.on_leave)
        self.da.connect("button-press-event", self.on_click)
        self.add(self.da)

        self.resize(self.win_w, self.win_h)
        self.update_geometry()
        screen.connect("size-changed", lambda *_: self.update_geometry())
        self.show_all()

        # Comprobacion inteligente cada 220 ms (auto-hide sin tapar ventanas!)
        GLib.timeout_add(220, self.check_intellihide)

    def get_geom(self):
        sc = self.get_screen()
        return sc.get_monitor_geometry(sc.get_primary_monitor())

    def update_geometry(self):
        g = self.get_geom()
        x = g.x + (g.width - self.win_w) // 2
        if self.hidden:
            # Dejar solo una franja sensible invisible de 5px en el borde inferior
            self.resize(self.win_w, 5)
            self.move(x, g.y + g.height - 5)
        else:
            self.resize(self.win_w, self.win_h)
            self.move(x, g.y + g.height - self.win_h - 10)
        self.da.queue_draw()

    def set_hidden(self, hide):
        if hide != self.hidden:
            self.hidden = hide
            if hide:
                self.hovered = -1
            self.update_geometry()

    def check_intellihide(self):
        g = self.get_geom()
        # Verificar posicion real del cursor
        try:
            seat = self.get_display().get_default_seat()
            pointer = seat.get_pointer()
            _, px, py = pointer.get_position()
        except Exception:
            px, py = 0, 0

        near_bottom = py >= (g.y + g.height - 8)
        in_dock_band = (not self.hidden) and py >= (g.y + g.height - self.win_h - 16) and (
            g.x + (g.width - self.win_w) // 2 <= px <= g.x + (g.width + self.win_w) // 2
        )

        if near_bottom or self.pointer_inside or in_dock_band:
            self.set_hidden(False)
            return True

        dock_top_y = g.y + g.height - self.pill_h - 16
        overlaps = any_window_overlaps_dock(dock_top_y, g.width, g.height)
        self.set_hidden(overlaps)
        return True

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

        if self.hidden:
            # Sutil indicador de 3px centrado en el borde inferior (como la barra home de macOS/iOS)
            bar_w = 110
            bx = (self.win_w - bar_w) / 2
            self.rounded_rect(cr, bx, 1, bar_w, 3, 1.5)
            cr.set_source_rgba(1, 1, 1, 0.35)
            cr.fill()
            return

        px = (self.win_w - self.pill_w) / 2
        py = self.top_space

        self.rounded_rect(cr, px + 2, py + 3, self.pill_w - 4, self.pill_h, 16)
        cr.set_source_rgba(0, 0, 0, 0.25)
        cr.fill()

        self.rounded_rect(cr, px, py, self.pill_w, self.pill_h, 16)
        cr.set_source_rgba(0.08, 0.09, 0.12, 0.46)
        cr.fill_preserve()
        cr.set_source_rgba(1, 1, 1, 0.24)
        cr.set_line_width(1.1)
        cr.stroke()

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

            if is_hov:
                cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
                cr.set_font_size(11.5)
                ext = cr.text_extents(label)
                tw = ext.width + 18
                th = 22
                tx = max(4, min(self.win_w - tw - 4, ix + self.icon_size / 2 - tw / 2))
                ty = py - th - 8
                self.rounded_rect(cr, tx, ty, tw, th, 8)
                cr.set_source_rgba(0.07, 0.09, 0.12, 0.94)
                cr.fill_preserve()
                cr.set_source_rgba(1, 1, 1, 0.16)
                cr.set_line_width(1.0)
                cr.stroke()
                cr.set_source_rgba(0.96, 0.97, 0.99, 1.0)
                cr.move_to(tx + 9, ty + 15)
                cr.show_text(label)

    def hit_test(self, x, y):
        if self.hidden or y < self.top_space - 6 or y > self.top_space + self.pill_h + 4:
            return -1
        for i in range(len(ITEMS)):
            ix = self.item_x(i)
            if ix - 4 <= x <= ix + self.icon_size + 4:
                return i
        return -1

    def on_enter(self, widget, ev):
        self.pointer_inside = True
        if self.hidden:
            self.set_hidden(False)

    def on_motion(self, widget, ev):
        if self.hidden:
            self.set_hidden(False)
            return
        h = self.hit_test(ev.x, ev.y)
        if h != self.hovered:
            self.hovered = h
            self.da.queue_draw()

    def on_leave(self, widget, ev):
        self.pointer_inside = False
        if self.hovered != -1:
            self.hovered = -1
            self.da.queue_draw()

    def on_click(self, widget, ev):
        if self.hidden:
            self.set_hidden(False)
            return
        idx = self.hit_test(ev.x, ev.y)
        if 0 <= idx < len(ITEMS):
            subprocess.Popen(ITEMS[idx][2], start_new_session=True)


if __name__ == "__main__":
    NeoDock()
    Gtk.main()
