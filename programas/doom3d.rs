// NeoDoom 3D — Motor FPS Raycaster 3D nativo en punto fijo (0 floats, 60+ FPS en Ring 3).
// Controles: W/S/Arriba/Abajo mover, A/D/Izq/Der girar, Espacio disparar (con audio AC97!).
use crate::gfx::{mezclar, Lienzo, H, W};

pub const VIEW_W: usize = 540;
pub const VIEW_H: usize = 340;

const MAP_W: usize = 12;
const MAP_H: usize = 12;

// 1 = pared piedra ciber, 2 = panel azul neon, 3 = pilar ambar, 4 = compuerta purpura
const MAPA: [[u8; MAP_W]; MAP_H] = [
    [1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1],
    [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 3, 0, 0, 0, 0, 0, 0, 2, 0, 1],
    [1, 0, 0, 0, 0, 4, 4, 0, 0, 0, 0, 1],
    [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2],
    [2, 0, 0, 3, 0, 0, 0, 0, 3, 0, 0, 2],
    [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2],
    [1, 0, 0, 0, 2, 0, 0, 2, 0, 0, 0, 1],
    [1, 0, 4, 0, 0, 0, 0, 0, 0, 3, 0, 1],
    [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    [1, 1, 1, 1, 2, 2, 2, 2, 1, 1, 1, 1],
];

// Tabla de seno en punto fijo (256 pasos = 360 grados, escala * 1024)
const SIN_64: [i32; 65] = [
    0, 25, 50, 75, 100, 125, 150, 175, 200, 224, 249, 273, 297, 321, 345, 368,
    392, 415, 438, 460, 483, 505, 526, 548, 569, 589, 610, 630, 649, 668, 687, 705,
    724, 741, 758, 775, 791, 807, 822, 837, 851, 865, 878, 891, 903, 915, 926, 936,
    946, 955, 964, 972, 979, 986, 992, 998, 1003, 1007, 1011, 1015, 1018, 1020, 1022, 1023,
    1024,
];

#[inline(always)]
fn sin_fp(ang: i32) -> i32 {
    let a = ang.rem_euclid(256) as usize;
    if a <= 64 {
        SIN_64[a]
    } else if a <= 128 {
        SIN_64[128 - a]
    } else if a <= 192 {
        -SIN_64[a - 128]
    } else {
        -SIN_64[256 - a]
    }
}

#[inline(always)]
fn cos_fp(ang: i32) -> i32 {
    sin_fp(ang + 64)
}

#[derive(Clone, Copy)]
pub struct Dron {
    pub x: i32, // punto fijo << 10
    pub y: i32,
    pub vivo: bool,
}

pub struct Motor3D {
    pub px: i32,      // posicion jugador << 10 (1024 = 1 celda)
    pub py: i32,
    pub ang: i32,     // 0..255 (256 = 360 grados)
    pub destello: u8, // frames de destello al disparar
    pub puntaje: u32,
    pub drones: [Dron; 4],
}

impl Motor3D {
    pub fn nuevo() -> Self {
        Motor3D {
            // Spawn en el gran salon central mirando directo al norte hacia los pilares y drones
            px: 5 * 1024 + 512,
            py: 9 * 1024 + 512,
            ang: 192, // mirando hacia -Y (norte) con vista simetrica abierta
            destello: 0,
            puntaje: 0,
            drones: [
                Dron { x: 5 * 1024 + 512, y: 5 * 1024 + 700, vivo: true },
                Dron { x: 8 * 1024 + 512, y: 2 * 1024 + 512, vivo: true },
                Dron { x: 9 * 1024 + 512, y: 7 * 1024 + 512, vivo: true },
                Dron { x: 3 * 1024 + 512, y: 4 * 1024 + 512, vivo: true },
            ],
        }
    }

    fn celda(gx: i32, gy: i32) -> u8 {
        if gx < 0 || gy < 0 || gx >= MAP_W as i32 || gy >= MAP_H as i32 {
            return 1;
        }
        MAPA[gy as usize][gx as usize]
    }

    fn libre(x: i32, y: i32) -> bool {
        let m = 240; // radio de colision para nunca pegarse a la pared
        Self::celda((x - m) >> 10, (y - m) >> 10) == 0
            && Self::celda((x + m) >> 10, (y - m) >> 10) == 0
            && Self::celda((x - m) >> 10, (y + m) >> 10) == 0
            && Self::celda((x + m) >> 10, (y + m) >> 10) == 0
    }

    pub fn mover(&mut self, adelante: i32) {
        let paso = 200;
        let nx = self.px + (cos_fp(self.ang) * paso * adelante) / 1024;
        let ny = self.py + (sin_fp(self.ang) * paso * adelante) / 1024;
        if Self::libre(nx, self.py) {
            self.px = nx;
        }
        if Self::libre(self.px, ny) {
            self.py = ny;
        }
    }

    pub fn girar(&mut self, delta: i32) {
        self.ang = (self.ang + delta).rem_euclid(256);
    }

    /// Dispara un rayo central; devuelve `true` si acerto a un dron objetivo.
    pub fn disparar(&mut self) -> bool {
        self.destello = 3;
        let dx = cos_fp(self.ang);
        let dy = sin_fp(self.ang);
        let mut impacto = false;

        for d in self.drones.iter_mut() {
            if !d.vivo { continue; }
            let vx = d.x - self.px;
            let vy = d.y - self.py;
            let proj = (vx * dx + vy * dy) >> 10;
            if proj > 256 && proj < 9 * 1024 {
                let perp = ((vx * dy - vy * dx) >> 10).abs();
                if perp < 420 {
                    d.vivo = false;
                    self.puntaje += 100;
                    impacto = true;
                    break;
                }
            }
        }

        // Si destruyo todos los drones, reaparece una nueva oleada
        if self.drones.iter().all(|d| !d.vivo) {
            for d in self.drones.iter_mut() {
                d.vivo = true;
            }
        }
        impacto
    }

    pub fn dibujar(&self, l: &mut Lienzo, ox: i32, oy: i32, acento: u32) {
        let vw = VIEW_W as i32;
        let vh = VIEW_H as i32;
        let mitad_h = vh / 2;

        // 1) Cielo oscuro degradado + suelo metalico con profundidad
        for py in 0..vh {
            let sy = oy + py;
            if sy < 0 || sy >= H as i32 { continue; }
            let col = if py < mitad_h {
                let t = ((py * 255) / mitad_h) as u32;
                mezclar(0x0B101B, 0x161E2E, t)
            } else {
                let t = (((py - mitad_h) * 255) / mitad_h) as u32;
                mezclar(0x131720, 0x262D3B, t)
            };
            let base = (sy as usize) * W;
            for px in 0..vw {
                let sx = ox + px;
                if sx >= 0 && sx < W as i32 {
                    l.px[base + sx as usize] = col;
                }
            }
        }

        let mut zbuf = [32767i32; VIEW_W];
        let dir_x = cos_fp(self.ang);
        let dir_y = sin_fp(self.ang);
        // Plano de camara perpendicular (FOV ~66 grados => escala 0.66 = * 680 / 1024)
        let plane_x = (-dir_y * 680) >> 10;
        let plane_y = (dir_x * 680) >> 10;

        // 2) Raycasting suave por plano de camara (270 rayos nitidos sin escalonado)
        let mut col_x = 0usize;
        while col_x < VIEW_W {
            // cam_x en punto fijo [-1024 .. +1024]
            let cam_x = ((col_x as i32) * 2048) / vw - 1024;
            let rdx = dir_x + ((plane_x * cam_x) >> 10);
            let rdy = dir_y + ((plane_y * cam_x) >> 10);

            let mut rx = self.px;
            let mut ry = self.py;
            let mut t_dist = 0i32;
            let mut pared = 0u8;
            let mut paso = 32i32;

            for _ in 0..280 {
                rx += (rdx * paso) >> 10;
                ry += (rdy * paso) >> 10;
                t_dist += paso;
                let c = Self::celda(rx >> 10, ry >> 10);
                if c != 0 {
                    rx -= (rdx * paso) >> 10;
                    ry -= (rdy * paso) >> 10;
                    t_dist -= paso;
                    paso = 6;
                    for _ in 0..6 {
                        rx += (rdx * paso) >> 10;
                        ry += (rdy * paso) >> 10;
                        t_dist += paso;
                        let c2 = Self::celda(rx >> 10, ry >> 10);
                        if c2 != 0 {
                            pared = c2;
                            break;
                        }
                    }
                    if pared == 0 { pared = c; }
                    break;
                }
            }

            // Con vector de plano de camara, t_dist YA es distancia perpendicular sin ojo de pez!
            let corr = t_dist.max(160);
            zbuf[col_x] = corr;
            if col_x + 1 < VIEW_W {
                zbuf[col_x + 1] = corr;
            }

            if pared != 0 {
                let altura = ((vh * 1024) / corr).min(vh * 2);
                let y0 = (mitad_h - altura / 2).max(0);
                let y1 = (mitad_h + altura / 2).min(vh - 1);

                // Coordenada de textura horizontal (0..15)
                let fx = (rx & 1023) >> 6;
                let fy = (ry & 1023) >> 6;
                let es_borde_x = fx == 0 || fx == 15;
                let es_borde_y = fy == 0 || fy == 15;
                let tex_u = if es_borde_x { fy } else { fx };

                let base_col = match pared {
                    1 => 0x64748B, // piedra tech
                    2 => 0x0EA5E9, // cian neon
                    3 => 0xF59E0B, // ambar
                    _ => 0xA855F7, // purpura
                };
                // Sombreado direccional + niebla de distancia
                let niebla = ((corr * 220) / (10 * 1024)).clamp(0, 210) as u32;
                let sombreado = if es_borde_x && !es_borde_y {
                    mezclar(base_col, 0x090D16, 65)
                } else {
                    base_col
                };
                let col_pared = mezclar(sombreado, 0x0D111A, niebla);
                let col_linea = mezclar(col_pared, 0x05070B, 110);

                for py in y0..=y1 {
                    let sy = oy + py;
                    if sy < 0 || sy >= H as i32 { continue; }
                    let v = (((py - (mitad_h - altura / 2)) * 16) / altura.max(1)) & 15;
                    let c = if v == 0 || v == 15 || tex_u == 0 || (v == 8 && (tex_u & 3) == 0) {
                        col_linea
                    } else if pared == 2 && v == 7 && tex_u > 3 && tex_u < 12 {
                        mezclar(0x7DD3FC, 0x0D111A, niebla / 2)
                    } else {
                        col_pared
                    };
                    let row = (sy as usize) * W;
                    let sx = ox + col_x as i32;
                    if sx >= 0 && sx + 1 < W as i32 {
                        l.px[row + sx as usize] = c;
                        l.px[row + sx as usize + 1] = c;
                    }
                }
            }
            col_x += 2;
        }

        // 3) Dibujar drones enemigos en 3D con Z-buffer
        let fdx = cos_fp(self.ang);
        let fdy = sin_fp(self.ang);
        for d in &self.drones {
            if !d.vivo { continue; }
            let vx = d.x - self.px;
            let vy = d.y - self.py;
            let prof = (vx * fdx + vy * fdy) >> 10;
            if prof < 320 || prof > 10 * 1024 { continue; }
            let lat = (vx * (-fdy) + vy * fdx) >> 10;
            let sx_cen = (vw / 2) + (lat * (vw / 2)) / prof;
            let rad = ((vh * 260) / prof).clamp(6, 72);
            let sy_cen = mitad_h;
            for dy in -rad..=rad {
                for dx in -rad..=rad {
                    let px = sx_cen + dx;
                    let py = sy_cen + dy;
                    if px < 0 || px >= vw || py < 0 || py >= vh { continue; }
                    if prof >= zbuf[px as usize] { continue; }
                    let r2 = dx * dx + dy * dy;
                    if r2 <= rad * rad {
                        let col = if r2 <= (rad / 3) * (rad / 3) {
                            0xF8FAFC // nucleo brillante
                        } else if r2 <= (rad * 2 / 3) * (rad * 2 / 3) {
                            0xF43F5E // anillo rojo cyber
                        } else {
                            0x4C1D95 // carcasa oscura
                        };
                        l.poner(ox + px, oy + py, col);
                    }
                }
            }
        }

        // 4) Mira central + arma minimalista + destello de disparo
        let cx = ox + vw / 2;
        let cy = oy + mitad_h;
        l.rect(cx - 8, cy, 17, 1, 0x4ADE80);
        l.rect(cx, cy - 8, 1, 17, 0x4ADE80);

        if self.destello > 0 {
            l.circulo_aa(cx, oy + vh - 62, 24, 0x38BDF8, 180);
            l.circulo_aa(cx, oy + vh - 62, 12, 0xFFFFFF, 240);
        }
        l.rect_redondo(cx - 26, oy + vh - 58, 52, 58, 10, 0x1E293B, 255);
        l.rect_redondo(cx - 10, oy + vh - 70, 20, 28, 6, acento, 245);

        // 5) Minimapa HUD en la esquina superior izquierda
        let mmx = ox + 12;
        let mmy = oy + 12;
        l.rect_redondo(mmx - 4, mmy - 4, (MAP_W as i32) * 7 + 8, (MAP_H as i32) * 7 + 8, 8, 0x05070B, 175);
        for gy in 0..MAP_H {
            for gx in 0..MAP_W {
                if MAPA[gy][gx] != 0 {
                    l.rect(mmx + (gx as i32) * 7, mmy + (gy as i32) * 7, 6, 6, 0x475569);
                }
            }
        }
        for d in &self.drones {
            if d.vivo {
                l.rect(mmx + (d.x >> 10) * 7 + 1, mmy + (d.y >> 10) * 7 + 1, 4, 4, 0xF43F5E);
            }
        }
        let jx = mmx + (self.px * 7) / 1024;
        let jy = mmy + (self.py * 7) / 1024;
        l.circulo_aa(jx, jy, 3, 0x4ADE80, 255);
    }
}
