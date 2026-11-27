// Motor grafico suave 2D (1280x720): esquinas redondeadas anti-aliased (4x4 supersampling),
// tipografia anti-aliased de 8 bits, cristal esmerilado con blur real y 4 wallpapers nativos HD.
use crate::fuentes_suaves::{
    DISP_ADV, DISP_CHARS, DISP_DATA, DISP_H, DISP_W, MONO_DATA, MONO_H, MONO_W, UI_ADV, UI_DATA,
    UI_H, UI_W,
};

pub const W: usize = 1280;
pub const H: usize = 720;
pub const WALL_W: usize = 640;
pub const WALL_H: usize = 360;

#[inline(always)]
pub fn mezclar(fondo: u32, frente: u32, alpha: u32) -> u32 {
    if alpha == 0 {
        return fondo;
    }
    if alpha >= 255 {
        return frente;
    }
    let inv = 255 - alpha;
    let r = ((((fondo >> 16) & 0xFF) * inv + ((frente >> 16) & 0xFF) * alpha) >> 8) & 0xFF;
    let g = ((((fondo >> 8) & 0xFF) * inv + ((frente >> 8) & 0xFF) * alpha) >> 8) & 0xFF;
    let b = (((fondo & 0xFF) * inv + (frente & 0xFF) * alpha) >> 8) & 0xFF;
    (r << 16) | (g << 8) | b
}

#[inline(always)]
pub fn escalar_brillo(col: u32, brillo: u32) -> u32 {
    if brillo >= 100 {
        return col;
    }
    let r = (((col >> 16) & 0xFF) * brillo / 100).min(255);
    let g = (((col >> 8) & 0xFF) * brillo / 100).min(255);
    let b = ((col & 0xFF) * brillo / 100).min(255);
    (r << 16) | (g << 8) | b
}

#[inline(always)]
fn rgb565_a_rgb888(c: u16) -> u32 {
    let r = (((c >> 11) & 0x1F) as u32) * 255 / 31;
    let g = (((c >> 5) & 0x3F) as u32) * 255 / 63;
    let b = ((c & 0x1F) as u32) * 255 / 31;
    (r << 16) | (g << 8) | b
}

/// Seno rapido en punto flotante (aproximacion Bhaskara/polinomial continua, sin libm).
#[inline(always)]
fn sin_aprox(mut x: f32) -> f32 {
    const TWO_PI: f32 = 6.2831853;
    const PI: f32 = 3.14159265;
    // Reducimos a [-PI, PI]
    let k = (x / TWO_PI) as i32;
    x -= (k as f32) * TWO_PI;
    if x > PI { x -= TWO_PI; }
    if x < -PI { x += TWO_PI; }
    // Aproximacion de precision alta: 1.27323954*x - 0.405284735*x*|x| refinada
    let y = 1.27323954 * x - 0.405284735 * x * if x < 0.0 { -x } else { x };
    0.225 * (y * if y < 0.0 { -y } else { y } - y) + y
}

#[inline(always)]
fn cos_aprox(x: f32) -> f32 {
    sin_aprox(x + 1.5707963)
}

#[inline(always)]
fn clamp01(v: f32) -> f32 {
    if v < 0.0 { 0.0 } else if v > 1.0 { 1.0 } else { v }
}

/// Genera uno de los 4 wallpapers nativos 1280x720 (sin pixelado).
pub fn generar_wallpaper(indice: usize, wall: &mut [u32], leer_foto: fn(&mut [u8]) -> usize, tmp: &mut [u32]) {
    match indice % 4 {
        0 => wallpaper_ondas_seda(wall),
        1 => wallpaper_montanas_niebla(wall),
        2 => wallpaper_hexagonos(wall),
        _ => {
            let raw = unsafe {
                core::slice::from_raw_parts_mut(tmp.as_mut_ptr() as *mut u8, WALL_W * WALL_H * 2)
            };
            let n = leer_foto(raw);
            if n >= WALL_W * WALL_H * 2 {
                escalar_rgb565_2x(raw, wall);
            } else {
                wallpaper_ondas_seda(wall);
            }
        }
    }
}

/// Wallpaper 0: Ondas de seda monocromaticas oscuras 3D (inspirado en el 00:00 de tu video!).
fn wallpaper_ondas_seda(wall: &mut [u32]) {
    for y in 0..H {
        let fy = (y as f32) / (H as f32);
        let base = y * W;
        // Cielo superior gris seda suave que baja hacia grafito oscuro
        let fondo_v = (168.0 - fy * 140.0).max(16.0);
        for x in 0..W {
            let fx = (x as f32) / (W as f32);

            // 4 pliegues de seda superpuestos con iluminacion especular suave
            let w1 = 0.56 + 0.08 * sin_aprox(fx * 5.2 + 0.6) + 0.04 * cos_aprox(fx * 2.7);
            let w2 = 0.65 + 0.09 * sin_aprox(fx * 4.3 + 2.3) + 0.03 * sin_aprox(fx * 8.1);
            let w3 = 0.76 + 0.07 * sin_aprox(fx * 6.0 + 4.1) + 0.04 * cos_aprox(fx * 3.1 + 1.0);
            let w4 = 0.88 + 0.06 * sin_aprox(fx * 3.8 + 0.9);

            // Brillo sutil del fondo superior (viñeta suave hacia bordes)
            let dx = fx - 0.45;
            let vignette = 1.0 - 0.25 * dx * dx;
            let mut lum = fondo_v * vignette;

            // Pliegue 1 (seda gris media con luz superior en la cresta)
            if fy > w1 {
                let d = (fy - w1) * 9.5;
                let cresta = 1.0 / (1.0 + d * d * 2.2);
                let sombra = clamp01(1.0 - d * 0.55);
                lum = 28.0 + 132.0 * cresta + 44.0 * sombra * (0.5 + 0.5 * cos_aprox(fx * 4.0 + fy * 3.0));
            }
            // Pliegue 2 (onda principal brillante con caida oscura profunda)
            if fy > w2 {
                let d = (fy - w2) * 8.5;
                let cresta = 1.0 / (1.0 + d * d * 1.8);
                let onda = 0.5 + 0.5 * sin_aprox(fx * 5.5 - fy * 4.0 + 1.2);
                lum = 18.0 + 148.0 * cresta + 36.0 * onda * clamp01(1.0 - d * 0.6);
            }
            // Pliegue 3 (satin negro profundo con reflejo plateado)
            if fy > w3 {
                let d = (fy - w3) * 10.0;
                let cresta = 1.0 / (1.0 + d * d * 2.5);
                lum = 12.0 + 115.0 * cresta + 26.0 * clamp01(1.0 - d * 0.5);
            }
            // Pliegue 4 (base carbon suave)
            if fy > w4 {
                let d = (fy - w4) * 12.0;
                let cresta = 1.0 / (1.0 + d * d * 2.8);
                lum = 8.0 + 90.0 * cresta;
            }

            let v = (lum as i32).clamp(6, 225) as u32;
            // Tinte calido-platino ultra sutil (monocromatico elegante)
            let r = v;
            let g = v;
            let b = (v + 3).min(255);
            wall[base + x] = (r << 16) | (g << 8) | b;
        }
    }
}

/// Wallpaper 1: Montanas alpinas con niebla monocromatica (como el 00:04 de tu video!).
fn wallpaper_montanas_niebla(wall: &mut [u32]) {
    // Precalculamos 5 perfiles de cordillera por columna X
    let mut r1 = [0i32; W];
    let mut r2 = [0i32; W];
    let mut r3 = [0i32; W];
    let mut r4 = [0i32; W];
    for x in 0..W {
        let fx = (x as f32) / (W as f32);
        // Pico principal majestuoso ligeramente a la derecha
        let pico = {
            let d = (fx - 0.58) * 3.1;
            let a = if d < 0.0 { -d } else { d };
            (1.0 - a).max(0.0)
        };
        let n1 = 0.05 * sin_aprox(fx * 11.0) + 0.025 * cos_aprox(fx * 27.0) + 0.012 * sin_aprox(fx * 53.0);
        let n2 = 0.06 * sin_aprox(fx * 7.5 + 1.4) + 0.03 * cos_aprox(fx * 19.0 + 0.5) + 0.01 * sin_aprox(fx * 41.0);
        let n3 = 0.05 * cos_aprox(fx * 5.8 + 2.7) + 0.025 * sin_aprox(fx * 15.0) + 0.01 * cos_aprox(fx * 37.0);
        let n4 = 0.04 * sin_aprox(fx * 4.2 + 0.9) + 0.02 * cos_aprox(fx * 13.0 + 2.1);

        r1[x] = ((0.56 - pico * 0.34 + n1) * (H as f32)) as i32;
        r2[x] = ((0.64 - pico * 0.16 + n2) * (H as f32)) as i32;
        r3[x] = ((0.74 + n3) * (H as f32)) as i32;
        r4[x] = ((0.84 + n4) * (H as f32)) as i32;
    }

    for y in 0..H {
        let fy = (y as f32) / (H as f32);
        let base = y * W;
        let cielo = 212.0 - fy * 62.0;
        let yi = y as i32;
        for x in 0..W {
            let fx = (x as f32) / (W as f32);
            let niebla_banda = 18.0 * sin_aprox(fx * 4.5 + fy * 3.0);
            let mut v = cielo + niebla_banda * 0.35;

            if yi >= r1[x] {
                let d = ((yi - r1[x]) as f32) / 140.0;
                v = 112.0 + clamp01(d) * 58.0; // cordillera lejana envuelta en niebla
            }
            if yi >= r2[x] {
                let d = ((yi - r2[x]) as f32) / 120.0;
                v = 78.0 + clamp01(d) * 44.0;
            }
            if yi >= r3[x] {
                let d = ((yi - r3[x]) as f32) / 100.0;
                v = 48.0 + clamp01(d) * 32.0;
            }
            if yi >= r4[x] {
                let d = ((yi - r4[x]) as f32) / 90.0;
                v = 24.0 + clamp01(d) * 18.0;
            }

            let c = (v as i32).clamp(12, 232) as u32;
            wall[base + x] = (c << 16) | (c << 8) | (c + 2).min(255);
        }
    }
}

/// Wallpaper 2: Malla hexagonal oscura 3D sombreada (como el 00:01 de tu video!).
fn wallpaper_hexagonos(wall: &mut [u32]) {
    const TAM: f32 = 46.0;
    const SQRT3: f32 = 1.7320508;
    for y in 0..H {
        let fy = y as f32;
        let base = y * W;
        for x in 0..W {
            let fx = x as f32;
            // Coordenadas en celda hexagonal repetitiva
            let qx = fx / (TAM * 1.5);
            let qy = fy / (TAM * SQRT3);
            let col_i = qx as i32;
            let off_y = if (col_i & 1) != 0 { 0.5 } else { 0.0 };
            let row_i = (qy + off_y) as i32;

            let cx = (col_i as f32 + 0.5) * (TAM * 1.5);
            let cy = (row_i as f32 + 0.5 - off_y) * (TAM * SQRT3);

            let dx = (fx - cx) / TAM;
            let dy = (fy - cy) / TAM;
            let adx = if dx < 0.0 { -dx } else { dx };
            let ady = if dy < 0.0 { -dy } else { dy };
            // Distancia hexagonal normalizada (0 en centro, 1 en borde)
            let hd = (adx * 0.866025 + ady * 0.5).max(ady);

            // Variacion sutil de altura por hexagono
            let hash = (((col_i * 73856093) ^ (row_i * 19349663)) & 0xFF) as f32 / 255.0;
            let luz_global = 1.0 - 0.45 * ((fx / W as f32 - 0.35) * (fx / W as f32 - 0.35)
                + (fy / H as f32 - 0.35) * (fy / H as f32 - 0.35));

            let mut lum = (36.0 + hash * 28.0 + (-dx * 16.0 - dy * 18.0)) * luz_global;
            // Borde biselado suave oscuro entre hexagonos
            if hd > 0.78 {
                let t = clamp01((hd - 0.78) / 0.08);
                lum = lum * (1.0 - t) + 10.0 * t;
            } else if hd > 0.70 {
                // Destello fino en el bisel superior
                lum += (1.0 - (hd - 0.70) / 0.08) * 10.0;
            }

            let v = (lum as i32).clamp(8, 120) as u32;
            wall[base + x] = (v << 16) | (v << 8) | (v + 4).min(255);
        }
    }
}

fn escalar_rgb565_2x(raw: &[u8], wall: &mut [u32]) {
    let leer = |sx: usize, sy: usize| -> u32 {
        let sx = sx.min(WALL_W - 1);
        let sy = sy.min(WALL_H - 1);
        let i = (sy * WALL_W + sx) * 2;
        rgb565_a_rgb888(u16::from_le_bytes([raw[i], raw[i + 1]]))
    };
    for sy in 0..WALL_H {
        let y0 = sy * 2;
        let y1 = y0 + 1;
        for sx in 0..WALL_W {
            let c00 = leer(sx, sy);
            let c10 = leer(sx + 1, sy);
            let c01 = leer(sx, sy + 1);
            let c11 = leer(sx + 1, sy + 1);
            let x0 = sx * 2;
            let x1 = x0 + 1;
            wall[y0 * W + x0] = c00;
            wall[y0 * W + x1] = mezclar(c00, c10, 128);
            wall[y1 * W + x0] = mezclar(c00, c01, 128);
            wall[y1 * W + x1] = mezclar(mezclar(c00, c10, 128), mezclar(c01, c11, 128), 128);
        }
    }
}

/// Box blur separable de 2 pasadas (radio 10 -> efecto cristal esmerilado suave tipo Hyprland/macOS).
pub fn generar_blur(wall: &[u32], blur_dst: &mut [u32], tmp: &mut [u32]) {
    const R: usize = 10;
    const DIV: u32 = (R * 2 + 1) as u32;

    blur_dst.copy_from_slice(wall);
    for _ in 0..2 {
        for y in 0..H {
            let base = y * W;
            let (mut sr, mut sg, mut sb) = (0u32, 0u32, 0u32);
            for i in 0..=R {
                let c = blur_dst[base + i];
                sr += (c >> 16) & 0xFF;
                sg += (c >> 8) & 0xFF;
                sb += c & 0xFF;
            }
            let c0 = blur_dst[base];
            sr += ((c0 >> 16) & 0xFF) * (R as u32);
            sg += ((c0 >> 8) & 0xFF) * (R as u32);
            sb += (c0 & 0xFF) * (R as u32);

            for x in 0..W {
                tmp[base + x] = ((sr / DIV) << 16) | ((sg / DIV) << 8) | (sb / DIV);
                let ox = x.saturating_sub(R);
                let ix = (x + R + 1).min(W - 1);
                let co = blur_dst[base + ox];
                let ci = blur_dst[base + ix];
                sr = sr + ((ci >> 16) & 0xFF) - ((co >> 16) & 0xFF);
                sg = sg + ((ci >> 8) & 0xFF) - ((co >> 8) & 0xFF);
                sb = sb + (ci & 0xFF) - (co & 0xFF);
            }
        }
        for x in 0..W {
            let (mut sr, mut sg, mut sb) = (0u32, 0u32, 0u32);
            for i in 0..=R {
                let c = tmp[i * W + x];
                sr += (c >> 16) & 0xFF;
                sg += (c >> 8) & 0xFF;
                sb += c & 0xFF;
            }
            let c0 = tmp[x];
            sr += ((c0 >> 16) & 0xFF) * (R as u32);
            sg += ((c0 >> 8) & 0xFF) * (R as u32);
            sb += (c0 & 0xFF) * (R as u32);

            for y in 0..H {
                blur_dst[y * W + x] = ((sr / DIV) << 16) | ((sg / DIV) << 8) | (sb / DIV);
                let oy = y.saturating_sub(R);
                let iy = (y + R + 1).min(H - 1);
                let co = tmp[oy * W + x];
                let ci = tmp[iy * W + x];
                sr = sr + ((ci >> 16) & 0xFF) - ((co >> 16) & 0xFF);
                sg = sg + ((ci >> 8) & 0xFF) - ((co >> 8) & 0xFF);
                sb = sb + (ci & 0xFF) - (co & 0xFF);
            }
        }
    }
}

/// Cobertura anti-aliased (0..255) en una esquina redondeada usando 4x4 subpixeles (cero pixelado).
#[inline(always)]
fn cobertura_esquina(lx: i32, ly: i32, r: i32) -> u32 {
    // Centro del arco esta en (r, r) en coordenadas locales de esquina [0..r)
    let r2_16 = (r * 4) * (r * 4);
    let cx = r * 4;
    let cy = r * 4;
    let mut dentro = 0u32;
    let bx = lx * 4;
    let by = ly * 4;
    for sy in 0..4 {
        let dy = cy - (by + sy);
        let dy2 = dy * dy;
        for sx in 0..4 {
            let dx = cx - (bx + sx);
            if dx * dx + dy2 <= r2_16 {
                dentro += 1;
            }
        }
    }
    (dentro * 255) / 16
}

pub struct Lienzo<'a> {
    pub px: &'a mut [u32],
    pub wall: &'a [u32],
    pub blur: &'a [u32],
}

impl<'a> Lienzo<'a> {
    #[inline(always)]
    pub fn poner(&mut self, x: i32, y: i32, col: u32) {
        if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H {
            self.px[(y as usize) * W + (x as usize)] = col;
        }
    }

    #[inline(always)]
    pub fn poner_alfa(&mut self, x: i32, y: i32, col: u32, alfa: u32) {
        if alfa == 0 { return; }
        if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H {
            let idx = (y as usize) * W + (x as usize);
            self.px[idx] = mezclar(self.px[idx], col, alfa);
        }
    }

    pub fn fondo(&mut self, desenfocado: bool) {
        if desenfocado {
            self.px.copy_from_slice(self.blur);
        } else {
            self.px.copy_from_slice(self.wall);
        }
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, col: u32) {
        let x0 = x.max(0) as usize;
        let y0 = y.max(0) as usize;
        let x1 = (x + w).min(W as i32).max(0) as usize;
        let y1 = (y + h).min(H as i32).max(0) as usize;
        if x0 >= x1 || y0 >= y1 { return; }
        for row in y0..y1 {
            let base = row * W;
            for cx in x0..x1 {
                self.px[base + cx] = col;
            }
        }
    }

    pub fn rect_alfa(&mut self, x: i32, y: i32, w: i32, h: i32, col: u32, alfa: u32) {
        let x0 = x.max(0) as usize;
        let y0 = y.max(0) as usize;
        let x1 = (x + w).min(W as i32).max(0) as usize;
        let y1 = (y + h).min(H as i32).max(0) as usize;
        if x0 >= x1 || y0 >= y1 { return; }
        for row in y0..y1 {
            let base = row * W;
            for cx in x0..x1 {
                self.px[base + cx] = mezclar(self.px[base + cx], col, alfa);
            }
        }
    }

    /// Rectangulo redondeado solido o semitransparente con esquinas anti-aliased suaves.
    pub fn rect_redondo(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, col: u32, alfa: u32) {
        if w <= 0 || h <= 0 || alfa == 0 { return; }
        let r = r.min(w / 2).min(h / 2).max(0);
        // Franja central completa (rapida)
        self.rect_alfa(x, y + r, w, h - 2 * r, col, alfa);
        // Franjas superior e inferior entre las esquinas
        self.rect_alfa(x + r, y, w - 2 * r, r, col, alfa);
        self.rect_alfa(x + r, y + h - r, w - 2 * r, r, col, alfa);
        // 4 esquinas anti-aliased
        for ly in 0..r {
            for lx in 0..r {
                let cov = cobertura_esquina(lx, ly, r);
                if cov == 0 { continue; }
                let a = (cov * alfa) / 255;
                self.poner_alfa(x + lx, y + ly, col, a);
                self.poner_alfa(x + w - 1 - lx, y + ly, col, a);
                self.poner_alfa(x + lx, y + h - 1 - ly, col, a);
                self.poner_alfa(x + w - 1 - lx, y + h - 1 - ly, col, a);
            }
        }
    }

    /// Panel de cristal esmerilado con esquinas redondeadas anti-aliased + blur real + borde sutil.
    pub fn panel_cristal(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        r: i32,
        tinte: u32,
        alfa_tinte: u32,
        con_blur: bool,
        borde_col: u32,
        borde_alfa: u32,
    ) {
        if w <= 0 || h <= 0 { return; }
        let r = r.min(w / 2).min(h / 2).max(0);
        let fuente = if con_blur { self.blur } else { self.wall };

        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(W as i32);
        let y1 = (y + h).min(H as i32);

        for py in y0..y1 {
            let base = (py as usize) * W;
            let ry_top = py - y;
            let ry_bot = (y + h - 1) - py;
            let en_banda_esq = ry_top < r || ry_bot < r;

            for px in x0..x1 {
                let idx = base + (px as usize);
                let vidrio = mezclar(fuente[idx], tinte, alfa_tinte);
                if !en_banda_esq {
                    self.px[idx] = vidrio;
                } else {
                    let rx_izq = px - x;
                    let rx_der = (x + w - 1) - px;
                    if rx_izq < r || rx_der < r {
                        let lx = rx_izq.min(rx_der);
                        let ly = ry_top.min(ry_bot);
                        let cov = cobertura_esquina(lx, ly, r);
                        if cov > 0 {
                            self.px[idx] = mezclar(self.px[idx], vidrio, cov);
                        }
                    } else {
                        self.px[idx] = vidrio;
                    }
                }
            }
        }

        if borde_alfa > 0 {
            self.borde_redondo(x, y, w, h, r, borde_col, borde_alfa);
        }
    }

    /// Borde fino de 1px siguiendo las esquinas redondeadas con anti-aliasing suave.
    pub fn borde_redondo(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, col: u32, alfa: u32) {
        let r = r.min(w / 2).min(h / 2).max(0);
        self.rect_alfa(x + r, y, w - 2 * r, 1, col, alfa);
        self.rect_alfa(x + r, y + h - 1, w - 2 * r, 1, col, alfa);
        self.rect_alfa(x, y + r, 1, h - 2 * r, col, alfa);
        self.rect_alfa(x + w - 1, y + r, 1, h - 2 * r, col, alfa);

        for ly in 0..r {
            for lx in 0..r {
                let c_out = cobertura_esquina(lx, ly, r);
                if c_out == 0 { continue; }
                let c_in = if lx > 0 && ly > 0 {
                    cobertura_esquina(lx - 1, ly - 1, (r - 1).max(1))
                } else {
                    0
                };
                let anillo = c_out.saturating_sub(c_in);
                if anillo > 0 {
                    let a = (anillo * alfa) / 255;
                    self.poner_alfa(x + lx, y + ly, col, a);
                    self.poner_alfa(x + w - 1 - lx, y + ly, col, a);
                    self.poner_alfa(x + lx, y + h - 1 - ly, col, a);
                    self.poner_alfa(x + w - 1 - lx, y + h - 1 - ly, col, a);
                }
            }
        }
    }

    /// Sombra suave difuminada solo en el halo exterior (30x mas rapida que rellenar el interior).
    pub fn sombra_suave(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32) {
        let capas = [
            (x - 4, y + 6, w + 8, h + 6, r + 4, 28u32),
            (x - 2, y + 4, w + 4, h + 4, r + 2, 42u32),
            (x, y + 2, w, h + 2, r, 55u32),
        ];
        for (sx, sy, sw, sh, sr, alfa) in capas {
            let sr = sr.min(sw / 2).min(sh / 2).max(0);
            // Borde inferior debajo de la ventana (parte mas visible de la sombra)
            let bot_y = (y + h - sr).max(sy);
            if bot_y < sy + sh {
                self.rect_alfa(sx + sr, bot_y, sw - 2 * sr, (sy + sh) - bot_y, 0x000000, alfa);
            }
            // Franjas laterales izquierda y derecha fuera de la ventana
            let izq_w = (x - sx).max(0);
            if izq_w > 0 {
                self.rect_alfa(sx, sy + sr, izq_w, sh - 2 * sr, 0x000000, alfa);
                self.rect_alfa(x + w, sy + sr, izq_w, sh - 2 * sr, 0x000000, alfa);
            }
            // 4 esquinas redondeadas anti-aliased
            for ly in 0..sr {
                for lx in 0..sr {
                    let cov = cobertura_esquina(lx, ly, sr);
                    if cov == 0 { continue; }
                    let a = (cov * alfa) / 255;
                    self.poner_alfa(sx + lx, sy + ly, 0x000000, a);
                    self.poner_alfa(sx + sw - 1 - lx, sy + ly, 0x000000, a);
                    self.poner_alfa(sx + lx, sy + sh - 1 - ly, 0x000000, a);
                    self.poner_alfa(sx + sw - 1 - lx, sy + sh - 1 - ly, 0x000000, a);
                }
            }
        }
    }

    /// Circulo relleno suave anti-aliased (para avatar, botones de ventana, toggles, hoy en calendario).
    pub fn circulo_aa(&mut self, cx: i32, cy: i32, radio: i32, col: u32, alfa: u32) {
        let r4 = radio * 4;
        let r2 = r4 * r4;
        for dy in -radio..=radio {
            for dx in -radio..=radio {
                let mut dentro = 0u32;
                for sy in 0..4 {
                    let y4 = dy * 4 + sy - 2;
                    for sx in 0..4 {
                        let x4 = dx * 4 + sx - 2;
                        if x4 * x4 + y4 * y4 <= r2 {
                            dentro += 1;
                        }
                    }
                }
                if dentro > 0 {
                    let a = (dentro * alfa) / 16;
                    self.poner_alfa(cx + dx, cy + dy, col, a);
                }
            }
        }
    }

    // --- Texto suave anti-aliased de 8 bits (UI proporcional, Monospace y Display grande) ---

    pub fn ancho_ui(s: &[u8]) -> i32 {
        let mut total = 0i32;
        for &b in s {
            if b == 0 { break; }
            if b >= 32 && b <= 126 {
                total += UI_ADV[(b - 32) as usize] as i32;
            }
        }
        total
    }

    pub fn texto_ui(&mut self, x: i32, y: i32, s: &[u8], col: u32) {
        let mut cx = x;
        for &b in s {
            if b == 0 { break; }
            if b < 32 || b > 126 { continue; }
            let gi = (b - 32) as usize;
            let off = gi * UI_W * UI_H;
            for gy in 0..UI_H {
                let py = y + gy as i32;
                if py < 0 || py >= H as i32 { continue; }
                let row_px = (py as usize) * W;
                let row_g = off + gy * UI_W;
                for gx in 0..UI_W {
                    let a = UI_DATA[row_g + gx] as u32;
                    if a == 0 { continue; }
                    let px = cx + gx as i32;
                    if px >= 0 && px < W as i32 {
                        let idx = row_px + px as usize;
                        self.px[idx] = mezclar(self.px[idx], col, a);
                    }
                }
            }
            cx += UI_ADV[gi] as i32;
        }
    }

    pub fn texto_ui_lim(&mut self, x: i32, y: i32, s: &[u8], max_w: i32, col: u32) {
        let mut cx = x;
        for &b in s {
            if b == 0 { break; }
            if b < 32 || b > 126 { continue; }
            let gi = (b - 32) as usize;
            let adv = UI_ADV[gi] as i32;
            if cx + adv > x + max_w { break; }
            let off = gi * UI_W * UI_H;
            for gy in 0..UI_H {
                let py = y + gy as i32;
                if py < 0 || py >= H as i32 { continue; }
                let row_px = (py as usize) * W;
                let row_g = off + gy * UI_W;
                for gx in 0..UI_W {
                    let a = UI_DATA[row_g + gx] as u32;
                    if a == 0 { continue; }
                    let px = cx + gx as i32;
                    if px >= 0 && px < W as i32 {
                        let idx = row_px + px as usize;
                        self.px[idx] = mezclar(self.px[idx], col, a);
                    }
                }
            }
            cx += adv;
        }
    }

    pub fn texto_ui_centrado(&mut self, cx: i32, y: i32, s: &[u8], col: u32) {
        let w = Self::ancho_ui(s);
        self.texto_ui(cx - w / 2, y, s, col);
    }

    pub fn texto_mono(&mut self, x: i32, y: i32, s: &[u8], max_chars: usize, col: u32) {
        let mut cx = x;
        let mut count = 0;
        for &b in s {
            if b == 0 || count >= max_chars { break; }
            if b >= 32 && b <= 126 {
                let off = ((b - 32) as usize) * MONO_W * MONO_H;
                for gy in 0..MONO_H {
                    let py = y + gy as i32;
                    if py < 0 || py >= H as i32 { continue; }
                    let row_px = (py as usize) * W;
                    let row_g = off + gy * MONO_W;
                    for gx in 0..MONO_W {
                        let a = MONO_DATA[row_g + gx] as u32;
                        if a == 0 { continue; }
                        let px = cx + gx as i32;
                        if px >= 0 && px < W as i32 {
                            let idx = row_px + px as usize;
                            self.px[idx] = mezclar(self.px[idx], col, a);
                        }
                    }
                }
            }
            cx += MONO_W as i32;
            count += 1;
        }
    }

    pub fn texto_disp_centrado(&mut self, cx: i32, y: i32, s: &[u8], espaciado: i32, col: u32, alfa_txt: u32) {
        let buscar = |b: u8| -> Option<usize> {
            let up = if b >= b'a' && b <= b'z' { b - 32 } else { b };
            DISP_CHARS.iter().position(|&c| c == up)
        };
        let mut total_w = 0i32;
        for (i, &b) in s.iter().enumerate() {
            if b == 0 { break; }
            if let Some(gi) = buscar(b) {
                total_w += DISP_ADV[gi] as i32;
                if i + 1 < s.len() && s[i + 1] != 0 {
                    total_w += espaciado;
                }
            }
        }
        let mut x = cx - total_w / 2;
        for &b in s {
            if b == 0 { break; }
            let Some(gi) = buscar(b) else { continue; };
            let off = gi * DISP_W * DISP_H;
            for gy in 0..DISP_H {
                let py = y + gy as i32;
                if py < 0 || py >= H as i32 { continue; }
                let row_px = (py as usize) * W;
                let row_g = off + gy * DISP_W;
                for gx in 0..DISP_W {
                    let a = ((DISP_DATA[row_g + gx] as u32) * alfa_txt) / 255;
                    if a == 0 { continue; }
                    let px = x + gx as i32;
                    if px >= 0 && px < W as i32 {
                        let idx = row_px + px as usize;
                        self.px[idx] = mezclar(self.px[idx], col, a);
                    }
                }
            }
            x += (DISP_ADV[gi] as i32) + espaciado;
        }
    }

    pub fn aplicar_brillo(&mut self, brillo: u32) {
        if brillo >= 100 { return; }
        for p in self.px.iter_mut() {
            *p = escalar_brillo(*p, brillo);
        }
    }
}

/// Restaura solo el parche 16x16 del cursor anterior desde `escena` hacia `fb` (0-lag).
pub fn restaurar_cursor_fb(fb: &mut [u32], escena: &[u32], ox: i32, oy: i32) {
    let x0 = ox.max(0) as usize;
    let y0 = oy.max(0) as usize;
    let x1 = (ox + 16).min(W as i32).max(0) as usize;
    let y1 = (oy + 16).min(H as i32).max(0) as usize;
    if x0 >= x1 || y0 >= y1 { return; }
    for row in y0..y1 {
        let base = row * W;
        fb[base + x0..base + x1].copy_from_slice(&escena[base + x0..base + x1]);
    }
}

/// Dibuja el cursor 16x16 directamente en `fb` sin redibujar el escritorio.
pub fn dibujar_cursor_en(fb: &mut [u32], mx: i32, my: i32) {
    const FORMA: [&[u8]; 16] = [
        b"X               ",
        b"XX              ",
        b"X.X             ",
        b"X..X            ",
        b"X...X           ",
        b"X....X          ",
        b"X.....X         ",
        b"X......X        ",
        b"X.......X       ",
        b"X........X      ",
        b"X.....XXXXX     ",
        b"X..X..X         ",
        b"X.X X..X        ",
        b"XX   X..X       ",
        b"      X..X      ",
        b"       XX       ",
    ];
    for (dy, fila) in FORMA.iter().enumerate() {
        let py = my + dy as i32;
        if py < 0 || py >= H as i32 { continue; }
        let base = (py as usize) * W;
        for (dx, &c) in fila.iter().enumerate() {
            let px = mx + dx as i32;
            if px < 0 || px >= W as i32 { continue; }
            let idx = base + px as usize;
            match c {
                b'X' => fb[idx] = mezclar(fb[idx], 0x05070B, 235),
                b'.' => fb[idx] = 0xF8FAFC,
                _ => {}
            }
        }
    }
}
