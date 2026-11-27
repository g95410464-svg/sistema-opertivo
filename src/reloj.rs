// Driver de reloj de tiempo real CMOS RTC (puertos 0x70/0x71) para fecha y hora reales.
use alloc::format;
use alloc::string::String;
use x86_64::instructions::port::Port;

#[derive(Clone, Copy)]
pub struct FechaHora {
    pub anio: u16,
    pub mes: u8,
    pub dia: u8,
    pub hora: u8,
    pub minuto: u8,
    pub segundo: u8,
    pub dia_semana: u8, // 0=DOM .. 6=SAB
}

unsafe fn leer_cmos(reg: u8) -> u8 {
    let mut p_idx: Port<u8> = Port::new(0x70);
    let mut p_dat: Port<u8> = Port::new(0x71);
    p_idx.write(reg);
    p_dat.read()
}

fn bcd_a_bin(v: u8) -> u8 {
    (v & 0x0F) + ((v >> 4) * 10)
}

/// Algoritmo de Sakamoto para dia de la semana (0=Domingo .. 6=Sabado).
fn calcular_dia_semana(mut y: i32, m: i32, d: i32) -> u8 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    if m < 3 {
        y -= 1;
    }
    let idx = (y + y / 4 - y / 100 + y / 400 + T[(m - 1).clamp(0, 11) as usize] + d) % 7;
    idx.rem_euclid(7) as u8
}

pub fn leer_rtc() -> FechaHora {
    unsafe {
        // Esperamos a que no haya actualizacion en progreso (bit 7 del registro 0x0A).
        for _ in 0..10_000 {
            if (leer_cmos(0x0A) & 0x80) == 0 {
                break;
            }
        }
        let mut seg = leer_cmos(0x00);
        let mut min = leer_cmos(0x02);
        let mut hor = leer_cmos(0x04);
        let mut dia = leer_cmos(0x07);
        let mut mes = leer_cmos(0x08);
        let mut anio = leer_cmos(0x09);
        let reg_b = leer_cmos(0x0B);

        if (reg_b & 0x04) == 0 {
            seg = bcd_a_bin(seg);
            min = bcd_a_bin(min);
            hor = bcd_a_bin(hor & 0x7F) | (hor & 0x80);
            dia = bcd_a_bin(dia);
            mes = bcd_a_bin(mes);
            anio = bcd_a_bin(anio);
        }
        if (reg_b & 0x02) == 0 && (hor & 0x80) != 0 {
            hor = ((hor & 0x7F) + 12) % 24;
        }
        let anio_full = 2000u16 + anio as u16;
        let mes_c = mes.clamp(1, 12);
        let dia_c = dia.clamp(1, 31);
        let ds = calcular_dia_semana(anio_full as i32, mes_c as i32, dia_c as i32);

        FechaHora {
            anio: anio_full,
            mes: mes_c,
            dia: dia_c,
            hora: hor % 24,
            minuto: min % 60,
            segundo: seg % 60,
            dia_semana: ds,
        }
    }
}

pub fn formatear_proc_time() -> String {
    let f = leer_rtc();
    let dias = ["SUNDAY", "MONDAY", "TUESDAY", "WEDNESDAY", "THURSDAY", "FRIDAY", "SATURDAY"];
    let meses = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} {} {} {}\n",
        f.anio,
        f.mes,
        f.dia,
        f.hora,
        f.minuto,
        f.segundo,
        dias[f.dia_semana as usize],
        meses[(f.mes - 1) as usize],
        f.dia_semana
    )
}
