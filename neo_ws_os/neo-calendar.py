#!/usr/bin/env python3
"""
neo-calendar: Panel flotante de Calendario y Notificaciones al hacer clic en la hora de Polybar
(identico al panel central de cristal oscuro de GNOME 49 / video @_snnx7).
"""
import gi
import cairo
import calendar
import datetime
import math
import os
import subprocess

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gtk, Gdk

MESES_ES = [
    "", "Enero", "Febrero", "Marzo", "Abril", "Mayo", "Junio",
    "Julio", "Agosto", "Septiembre", "Octubre", "Noviembre", "Diciembre",
]
DIAS_ES = ["D", "L", "M", "X", "J", "V", "S"]
PID_FILE = "/tmp/neo_calendar.pid"


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


class NeoCalendar(Gtk.Window):
    def __init__(self):
        super().__init__(type=Gtk.WindowType.TOPLEVEL)
        self.set_title("neo-calendar")
        self.set_decorated(False)
        self.set_app_paintable(True)
        self.set_keep_above(True)
        self.set_skip_taskbar_hint(True)
        self.set_skip_pager_hint(True)

        screen = self.get_screen()
        visual = screen.get_rgba_visual()
        if visual:
            self.set_visual(visual)

        self.w = 500
        self.h = 286
        mon = screen.get_primary_monitor()
        geom = screen.get_monitor_geometry(mon)
        self.move(geom.x + (geom.width - self.w) // 2, geom.y + 44)
        self.resize(self.w, self.h)

        today = datetime.date.today()
        self.today = today
        self.year = today.year
        self.month = today.month
        self.prev_rect = (0, 0, 0, 0)
        self.next_rect = (0, 0, 0, 0)

        self.da = Gtk.DrawingArea()
        self.da.add_events(Gdk.EventMask.BUTTON_PRESS_MASK)
        self.da.connect("draw", self.on_draw)
        self.da.connect("button-press-event", self.on_click)
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

    def on_draw(self, widget, cr):
        cr.set_operator(cairo.OPERATOR_SOURCE)
        cr.set_source_rgba(0, 0, 0, 0)
        cr.paint()
        cr.set_operator(cairo.OPERATOR_OVER)

        # Panel de cristal oscuro redondeado
        self.rounded_rect(cr, 2, 2, self.w - 4, self.h - 4, 18)
        cr.set_source_rgba(0.08, 0.09, 0.12, 0.94)
        cr.fill_preserve()
        cr.set_source_rgba(1, 1, 1, 0.18)
        cr.set_line_width(1.1)
        cr.stroke()

        # Divisor vertical central
        div_x = 196
        cr.set_source_rgba(1, 1, 1, 0.10)
        cr.set_line_width(1.0)
        cr.move_to(div_x, 20)
        cr.line_to(div_x, self.h - 20)
        cr.stroke()

        # --- Lado izquierdo: Notificaciones + fecha legible ---
        cr.arc(div_x / 2, 108, 26, 0, 2 * math.pi)
        cr.set_source_rgba(1, 1, 1, 0.07)
        cr.fill()
        cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
        cr.set_font_size(14)
        cr.set_source_rgba(0.23, 0.51, 0.96, 1.0)
        ext = cr.text_extents("✓")
        cr.move_to(div_x / 2 - ext.width / 2, 113)
        cr.show_text("✓")

        cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
        cr.set_font_size(12.5)
        cr.set_source_rgba(0.82, 0.85, 0.90, 0.95)
        t1 = "Sin notificaciones"
        e1 = cr.text_extents(t1)
        cr.move_to(div_x / 2 - e1.width / 2, 158)
        cr.show_text(t1)

        cr.set_font_size(10.5)
        cr.set_source_rgba(0.55, 0.60, 0.68, 0.9)
        t2 = f"neo_ws OS  •  {self.today.strftime('%d/%m/%Y')}"
        e2 = cr.text_extents(t2)
        cr.move_to(div_x / 2 - e2.width / 2, 180)
        cr.show_text(t2)

        # --- Lado derecho: Calendario mensual interactivo ---
        gx = div_x + 22
        gw = self.w - gx - 20
        title = f"{MESES_ES[self.month]} {self.year}"
        cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
        cr.set_font_size(13.5)
        cr.set_source_rgba(0.95, 0.96, 0.98, 1.0)
        cr.move_to(gx + 6, 36)
        cr.show_text(title)

        # Botones mes anterior (<) y mes siguiente (>)
        self.prev_rect = (self.w - 72, 18, 24, 24)
        self.next_rect = (self.w - 42, 18, 24, 24)
        for (bx, by, bw, bh), sym in ((self.prev_rect, "‹"), (self.next_rect, "›")):
            self.rounded_rect(cr, bx, by, bw, bh, 7)
            cr.set_source_rgba(1, 1, 1, 0.08)
            cr.fill()
            cr.set_source_rgba(0.88, 0.91, 0.96, 1.0)
            cr.set_font_size(15)
            cr.move_to(bx + 8, by + 17)
            cr.show_text(sym)

        # Cabecera D L M X J V S
        col_w = gw / 7.0
        cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
        cr.set_font_size(10.5)
        cr.set_source_rgba(0.55, 0.60, 0.68, 0.95)
        for idx, d in enumerate(DIAS_ES):
            cx = gx + idx * col_w + col_w / 2
            ext = cr.text_extents(d)
            cr.move_to(cx - ext.width / 2, 68)
            cr.show_text(d)

        # Dias del mes (empezando en Domingo)
        cal = calendar.Calendar(firstweekday=6)
        weeks = cal.monthdayscalendar(self.year, self.month)
        cr.select_font_face("Inter", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_NORMAL)
        cr.set_font_size(11.5)
        for r_idx, week in enumerate(weeks):
            cy = 98 + r_idx * 32
            for c_idx, day in enumerate(week):
                if day == 0:
                    continue
                cx = gx + c_idx * col_w + col_w / 2
                is_today = (
                    day == self.today.day
                    and self.month == self.today.month
                    and self.year == self.today.year
                )
                if is_today:
                    cr.arc(cx, cy - 4, 13, 0, 2 * math.pi)
                    cr.set_source_rgba(0.23, 0.51, 0.96, 1.0)
                    cr.fill()
                    cr.set_source_rgba(1, 1, 1, 1)
                else:
                    cr.set_source_rgba(0.88, 0.91, 0.96, 0.92)
                txt = str(day)
                ext = cr.text_extents(txt)
                cr.move_to(cx - ext.width / 2, cy)
                cr.show_text(txt)

    def on_click(self, widget, ev):
        px, py, pw, ph = self.prev_rect
        if px <= ev.x <= px + pw and py <= ev.y <= py + ph:
            if self.month == 1:
                self.month = 12
                self.year -= 1
            else:
                self.month -= 1
            self.da.queue_draw()
            return
        nx, ny, nw, nh = self.next_rect
        if nx <= ev.x <= nx + nw and ny <= ev.y <= ny + nh:
            if self.month == 12:
                self.month = 1
                self.year += 1
            else:
                self.month += 1
            self.da.queue_draw()
            return

    def on_key(self, widget, ev):
        if ev.keyval == Gdk.KEY_Escape:
            self.quit_clean()


if __name__ == "__main__":
    if toggle_existing():
        raise SystemExit(0)
    with open(PID_FILE, "w") as f:
        f.write(str(os.getpid()))
    NeoCalendar()
    Gtk.main()
