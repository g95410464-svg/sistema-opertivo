// Driver PCI de audio real Intel 82801AA AC97 (vendor 0x8086, device 0x2415) para `/dev/dsp`.
// Reproduce PCM estereo 16-bit a 48 kHz mediante DMA (Buffer Descriptor List).
use crate::memoria;
use core::sync::atomic::{fence, Ordering};
use spin::Mutex;
use x86_64::instructions::port::Port;

const PCI_ADDR: u16 = 0x0CF8;
const PCI_DATA: u16 = 0x0CFC;

// Registros Mixer (NAMBAR = BAR0)
const MIX_RESET: u16 = 0x00;
const MIX_MASTER_VOL: u16 = 0x02;
const MIX_PCM_VOL: u16 = 0x18;

// Registros Bus Master PCM Out (NABMBAR = BAR1)
const PO_BDBAR: u16 = 0x10; // Buffer Descriptor List Base Address (u32 phys)
const PO_LVI: u16 = 0x15;   // Last Valid Index (u8)
const PO_SR: u16 = 0x16;    // Status Register (u16)
const PO_CR: u16 = 0x1B;    // Control Register (u8)
const GLOB_CNT: u16 = 0x2C; // Global Control (u32)

const NUM_BUFS: usize = 8; // 8 * 4096 bytes = 32 KiB (~170 ms a 48 kHz estereo 16-bit)

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct BdlEntry {
    addr: u32,
    samples: u16, // numero de muestras de 16-bit (stereo = 2 muestras por frame)
    flags: u16,
}

struct EstadoAudio {
    _nambar: u16,
    nabmbar: u16,
    bdl_phys: u32,
    bdl: *mut BdlEntry,
    buf_phys: [u32; NUM_BUFS],
    buf_ptr: [*mut i16; NUM_BUFS],
}

unsafe impl Send for EstadoAudio {}

static AUDIO: Mutex<Option<EstadoAudio>> = Mutex::new(None);

unsafe fn pci_leer_u32(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let dir: u32 = 0x8000_0000
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    let mut p_addr: Port<u32> = Port::new(PCI_ADDR);
    let mut p_data: Port<u32> = Port::new(PCI_DATA);
    p_addr.write(dir);
    p_data.read()
}

unsafe fn pci_escribir_u32(bus: u8, slot: u8, func: u8, offset: u8, val: u32) {
    let dir: u32 = 0x8000_0000
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    let mut p_addr: Port<u32> = Port::new(PCI_ADDR);
    let mut p_data: Port<u32> = Port::new(PCI_DATA);
    p_addr.write(dir);
    p_data.write(val);
}

/// Detecta e inicializa la tarjeta Intel AC97 en PCI bus 0.
pub fn init() -> bool {
    let mut encontrado: Option<(u16, u16)> = None;
    for slot in 0..32u8 {
        let id = unsafe { pci_leer_u32(0, slot, 0, 0x00) };
        if id == 0xFFFF_FFFF {
            continue;
        }
        let vendor = (id & 0xFFFF) as u16;
        let device = (id >> 16) as u16;
        if vendor == 0x8086 && device == 0x2415 {
            // Habilitar Bus Master (bit 2) e I/O Space (bit 0)
            let cmd = unsafe { pci_leer_u32(0, slot, 0, 0x04) };
            unsafe { pci_escribir_u32(0, slot, 0, 0x04, cmd | 0x05) };

            let bar0 = unsafe { pci_leer_u32(0, slot, 0, 0x10) };
            let bar1 = unsafe { pci_leer_u32(0, slot, 0, 0x14) };
            let nambar = (bar0 & 0xFFFC) as u16;
            let nabmbar = (bar1 & 0xFFFC) as u16;
            if nambar != 0 && nabmbar != 0 {
                encontrado = Some((nambar, nabmbar));
                break;
            }
        }
    }

    let Some((nambar, nabmbar)) = encontrado else {
        return false;
    };

    let Some((bdl_phys, bdl_raw)) = memoria::asignar_frame_dma() else {
        return false;
    };
    if bdl_phys > u32::MAX as u64 {
        return false;
    }

    let mut buf_phys = [0u32; NUM_BUFS];
    let mut buf_ptr = [core::ptr::null_mut::<i16>(); NUM_BUFS];
    for i in 0..NUM_BUFS {
        let Some((p, ptr)) = memoria::asignar_frame_dma() else {
            return false;
        };
        if p > u32::MAX as u64 {
            return false;
        }
        buf_phys[i] = p as u32;
        buf_ptr[i] = ptr as *mut i16;
    }

    unsafe {
        // Cold reset AC97 + reiniciar mixer + ajustar volumen agradable
        let mut gcnt: Port<u32> = Port::new(nabmbar + GLOB_CNT);
        gcnt.write(0x02);
        for _ in 0..20_000 {
            core::hint::spin_loop();
        }
        let mut p_rst: Port<u16> = Port::new(nambar + MIX_RESET);
        let mut p_mvol: Port<u16> = Port::new(nambar + MIX_MASTER_VOL);
        let mut p_pvol: Port<u16> = Port::new(nambar + MIX_PCM_VOL);
        p_rst.write(0x0000);
        p_mvol.write(0x0606); // volumen moderado y limpio
        p_pvol.write(0x0606);
    }

    *AUDIO.lock() = Some(EstadoAudio {
        _nambar: nambar,
        nabmbar,
        bdl_phys: bdl_phys as u32,
        bdl: bdl_raw as *mut BdlEntry,
        buf_phys,
        buf_ptr,
    });
    true
}

pub fn disponible() -> bool {
    AUDIO.lock().is_some()
}

/// Reproduce un tono suave (onda triangular filtrada con envolvente exponencial) a 48 kHz.
pub fn reproducir_tono(freq_hz: u32, dur_ms: u32) {
    let guard = AUDIO.lock();
    let Some(a) = guard.as_ref() else {
        return;
    };
    let freq = freq_hz.clamp(80, 4000);
    let total_frames = ((dur_ms.clamp(25, 165) * 48_000) / 1000) as usize; // frames estereo
    let max_frames = NUM_BUFS * 1024;
    let frames = total_frames.min(max_frames);
    let bufs_usados = ((frames + 1023) / 1024).max(1).min(NUM_BUFS);

    // Sintetizar onda triangular suave con fade-in y fade-out para cero "clicks"
    let periodo = (48_000 / freq).max(4) as usize;
    let mitad = periodo / 2;
    let mut f_idx = 0usize;

    for b in 0..bufs_usados {
        let slice = unsafe { core::slice::from_raw_parts_mut(a.buf_ptr[b], 2048) };
        slice.fill(0);
        for s in 0..1024 {
            if f_idx >= frames {
                break;
            }
            let fase = f_idx % periodo;
            let tri: i32 = if fase < mitad {
                -9000 + ((fase as i32) * 18000) / (mitad as i32)
            } else {
                9000 - (((fase - mitad) as i32) * 18000) / (mitad as i32)
            };
            // Envolvente suave (ataque 150 frames, decaimiento lineal)
            let env_in = if f_idx < 150 { (f_idx as i32 * 256) / 150 } else { 256 };
            let env_out = ((frames - f_idx) as i32 * 256) / (frames as i32).max(1);
            let muestra = ((tri * env_in * env_out) >> 16) as i16;
            slice[s * 2] = muestra;
            slice[s * 2 + 1] = muestra;
            f_idx += 1;
        }
        unsafe {
            *a.bdl.add(b) = BdlEntry {
                addr: a.buf_phys[b],
                samples: 2048,
                flags: 0,
            };
        }
    }

    fence(Ordering::SeqCst);
    unsafe {
        let mut p_cr: Port<u8> = Port::new(a.nabmbar + PO_CR);
        let mut p_sr: Port<u16> = Port::new(a.nabmbar + PO_SR);
        let mut p_bdbar: Port<u32> = Port::new(a.nabmbar + PO_BDBAR);
        let mut p_lvi: Port<u8> = Port::new(a.nabmbar + PO_LVI);

        // Reset DMA PCM Out
        p_cr.write(0x02);
        for _ in 0..2000 {
            if (p_cr.read() & 0x02) == 0 {
                break;
            }
        }
        p_sr.write(0x1C); // limpiar banderas de estado
        p_bdbar.write(a.bdl_phys);
        p_lvi.write((bufs_usados - 1) as u8);
        p_cr.write(0x01); // Run Bus Master!
    }
}

/// Reproduce un acorde ambiental minimalista (tipo macOS/Linux moderno) al iniciar sesion.
pub fn reproducir_acorde_inicio() {
    let guard = AUDIO.lock();
    let Some(a) = guard.as_ref() else {
        return;
    };
    // Acorde armonico C5 (523 Hz) + G5 (784 Hz) en 8 buffers (~170 ms suaves)
    let frames = NUM_BUFS * 1024;
    let p1 = 48_000 / 523;
    let p2 = 48_000 / 784;
    let mut f_idx = 0usize;

    for b in 0..NUM_BUFS {
        let slice = unsafe { core::slice::from_raw_parts_mut(a.buf_ptr[b], 2048) };
        for s in 0..1024 {
            let tri = |fi: usize, per: usize| -> i32 {
                let m = per / 2;
                let ph = fi % per;
                if ph < m {
                    -5000 + ((ph as i32) * 10000) / (m as i32)
                } else {
                    5000 - (((ph - m) as i32) * 10000) / (m as i32)
                }
            };
            let v1 = tri(f_idx, p1);
            let v2 = if f_idx > 1400 { tri(f_idx, p2) } else { 0 };
            let env_in = if f_idx < 300 { (f_idx as i32 * 256) / 300 } else { 256 };
            let env_out = ((frames - f_idx) as i32 * 256) / (frames as i32);
            let m = (((v1 + v2) * env_in * env_out) >> 16) as i16;
            slice[s * 2] = m;
            slice[s * 2 + 1] = m;
            f_idx += 1;
        }
        unsafe {
            *a.bdl.add(b) = BdlEntry {
                addr: a.buf_phys[b],
                samples: 2048,
                flags: 0,
            };
        }
    }

    fence(Ordering::SeqCst);
    unsafe {
        let mut p_cr: Port<u8> = Port::new(a.nabmbar + PO_CR);
        let mut p_sr: Port<u16> = Port::new(a.nabmbar + PO_SR);
        let mut p_bdbar: Port<u32> = Port::new(a.nabmbar + PO_BDBAR);
        let mut p_lvi: Port<u8> = Port::new(a.nabmbar + PO_LVI);
        p_cr.write(0x02);
        for _ in 0..2000 {
            if (p_cr.read() & 0x02) == 0 {
                break;
            }
        }
        p_sr.write(0x1C);
        p_bdbar.write(a.bdl_phys);
        p_lvi.write((NUM_BUFS - 1) as u8);
        p_cr.write(0x01);
    }
}

/// Procesa comandos escritos en `/dev/dsp`: `chime`, `laser`, `hit`, `tone <hz> <ms>`.
pub fn procesar_comando_dsp(bytes: &[u8]) {
    let s = core::str::from_utf8(bytes).unwrap_or("").trim();
    if s == "chime" {
        reproducir_acorde_inicio();
    } else if s == "laser" {
        reproducir_tono(880, 65);
    } else if s == "hit" {
        reproducir_tono(310, 95);
    } else if let Some(rest) = s.strip_prefix("tone ") {
        let mut it = rest.split_whitespace();
        let hz = it.next().and_then(|v| v.parse::<u32>().ok()).unwrap_or(440);
        let ms = it.next().and_then(|v| v.parse::<u32>().ok()).unwrap_or(110);
        reproducir_tono(hz, ms);
    } else {
        reproducir_tono(523, 85);
    }
}
