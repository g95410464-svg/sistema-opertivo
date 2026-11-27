// Driver de discos duros virtuales ATA PIO (bus IDE primario, modo LBA28):
// - Drive 0 (/dev/sda): imagen de arranque (solo lectura desde usuario)
// - Drive 1 (/dev/sdb): disco de datos persistente NeoFS (lectura y escritura real en `neo_disk.img`)
use alloc::format;
use alloc::string::String;
use spin::Mutex;
use x86_64::instructions::port::Port;

const ATA_DATA: u16 = 0x1F0;
const ATA_SECTOR_COUNT: u16 = 0x1F2;
const ATA_LBA_LO: u16 = 0x1F3;
const ATA_LBA_MID: u16 = 0x1F4;
const ATA_LBA_HI: u16 = 0x1F5;
const ATA_DRIVE: u16 = 0x1F6;
const ATA_STATUS_CMD: u16 = 0x1F7;
const ATA_CTRL: u16 = 0x3F6;

const SR_BSY: u8 = 0x80;
const SR_DRQ: u8 = 0x08;
const SR_ERR: u8 = 0x01;

#[derive(Clone)]
pub struct InfoDisco {
    pub modelo: String,
    pub sectores: u32,
    pub firma_mbr_ok: bool,
}

static DISCO_SDA: Mutex<Option<InfoDisco>> = Mutex::new(None);
static DISCO_SDB: Mutex<Option<InfoDisco>> = Mutex::new(None);
static ATA_LOCK: Mutex<()> = Mutex::new(());

fn retardo_400ns() {
    let mut alt: Port<u8> = Port::new(ATA_CTRL);
    unsafe {
        let _ = alt.read();
        let _ = alt.read();
        let _ = alt.read();
        let _ = alt.read();
    }
}

fn esperar_no_ocupado() -> bool {
    let mut status: Port<u8> = Port::new(ATA_STATUS_CMD);
    for _ in 0..300_000 {
        let s = unsafe { status.read() };
        if s == 0xFF {
            return false;
        }
        if (s & SR_BSY) == 0 {
            return true;
        }
    }
    false
}

fn esperar_drq() -> bool {
    let mut status: Port<u8> = Port::new(ATA_STATUS_CMD);
    for _ in 0..300_000 {
        let s = unsafe { status.read() };
        if (s & SR_ERR) != 0 {
            return false;
        }
        if (s & SR_BSY) == 0 && (s & SR_DRQ) != 0 {
            return true;
        }
    }
    false
}

fn detectar_unidad(drive: u8) -> Option<InfoDisco> {
    let mut p_drive: Port<u8> = Port::new(ATA_DRIVE);
    let mut p_cnt: Port<u8> = Port::new(ATA_SECTOR_COUNT);
    let mut p_lo: Port<u8> = Port::new(ATA_LBA_LO);
    let mut p_mid: Port<u8> = Port::new(ATA_LBA_MID);
    let mut p_hi: Port<u8> = Port::new(ATA_LBA_HI);
    let mut p_cmd: Port<u8> = Port::new(ATA_STATUS_CMD);
    let mut p_ctrl: Port<u8> = Port::new(ATA_CTRL);
    let mut p_data: Port<u16> = Port::new(ATA_DATA);

    let sel = if drive == 0 { 0xA0 } else { 0xB0 };
    unsafe {
        p_ctrl.write(0x02); // nIEN=1: modo PIO por sondeo (sin IRQ 14).
        p_drive.write(sel);
        retardo_400ns();
        p_cnt.write(0);
        p_lo.write(0);
        p_mid.write(0);
        p_hi.write(0);
        p_cmd.write(0xEC); // IDENTIFY
        retardo_400ns();
    }

    let inicial = unsafe { p_cmd.read() };
    if inicial == 0 || inicial == 0xFF {
        return None;
    }
    if !esperar_no_ocupado() || !esperar_drq() {
        return None;
    }

    let mut id_words = [0u16; 256];
    for w in id_words.iter_mut() {
        *w = unsafe { p_data.read() };
    }

    let mut modelo_bytes = [0u8; 40];
    for i in 0..20 {
        let w = id_words[27 + i];
        modelo_bytes[i * 2] = (w >> 8) as u8;
        modelo_bytes[i * 2 + 1] = (w & 0xFF) as u8;
    }
    let modelo = core::str::from_utf8(&modelo_bytes)
        .unwrap_or("ATA Disk")
        .trim();
    let sectores = (id_words[60] as u32) | ((id_words[61] as u32) << 16);
    if sectores == 0 {
        return None;
    }

    let mut mbr = [0u8; 512];
    let firma_mbr_ok =
        leer_sector_sin_lock(drive, 0, &mut mbr) && mbr[510] == 0x55 && mbr[511] == 0xAA;

    Some(InfoDisco {
        modelo: String::from(modelo),
        sectores,
        firma_mbr_ok,
    })
}

/// Detecta `/dev/sda` (Master) y `/dev/sdb` (Slave persistente).
pub fn init() -> Option<InfoDisco> {
    let _g = ATA_LOCK.lock();
    let sda = detectar_unidad(0);
    let sdb = detectar_unidad(1);
    *DISCO_SDA.lock() = sda.clone();
    *DISCO_SDB.lock() = sdb;
    sda
}

pub fn info_sdb() -> Option<InfoDisco> {
    DISCO_SDB.lock().clone()
}

fn leer_sector_sin_lock(drive: u8, lba: u32, buf: &mut [u8; 512]) -> bool {
    if !esperar_no_ocupado() {
        return false;
    }
    let mut p_drive: Port<u8> = Port::new(ATA_DRIVE);
    let mut p_cnt: Port<u8> = Port::new(ATA_SECTOR_COUNT);
    let mut p_lo: Port<u8> = Port::new(ATA_LBA_LO);
    let mut p_mid: Port<u8> = Port::new(ATA_LBA_MID);
    let mut p_hi: Port<u8> = Port::new(ATA_LBA_HI);
    let mut p_cmd: Port<u8> = Port::new(ATA_STATUS_CMD);
    let mut p_data: Port<u16> = Port::new(ATA_DATA);

    let base_sel = if drive == 0 { 0xE0 } else { 0xF0 };
    unsafe {
        p_drive.write(base_sel | (((lba >> 24) & 0x0F) as u8));
        retardo_400ns();
        p_cnt.write(1);
        p_lo.write((lba & 0xFF) as u8);
        p_mid.write(((lba >> 8) & 0xFF) as u8);
        p_hi.write(((lba >> 16) & 0xFF) as u8);
        p_cmd.write(0x20); // READ SECTORS (LBA28)
        retardo_400ns();
    }

    if !esperar_drq() {
        return false;
    }
    for i in 0..256 {
        let w = unsafe { p_data.read() };
        buf[i * 2] = (w & 0xFF) as u8;
        buf[i * 2 + 1] = (w >> 8) as u8;
    }
    true
}

fn escribir_sector_sin_lock(drive: u8, lba: u32, buf: &[u8; 512]) -> bool {
    if !esperar_no_ocupado() {
        return false;
    }
    let mut p_drive: Port<u8> = Port::new(ATA_DRIVE);
    let mut p_cnt: Port<u8> = Port::new(ATA_SECTOR_COUNT);
    let mut p_lo: Port<u8> = Port::new(ATA_LBA_LO);
    let mut p_mid: Port<u8> = Port::new(ATA_LBA_MID);
    let mut p_hi: Port<u8> = Port::new(ATA_LBA_HI);
    let mut p_cmd: Port<u8> = Port::new(ATA_STATUS_CMD);
    let mut p_data: Port<u16> = Port::new(ATA_DATA);

    let base_sel = if drive == 0 { 0xE0 } else { 0xF0 };
    unsafe {
        p_drive.write(base_sel | (((lba >> 24) & 0x0F) as u8));
        retardo_400ns();
        p_cnt.write(1);
        p_lo.write((lba & 0xFF) as u8);
        p_mid.write(((lba >> 8) & 0xFF) as u8);
        p_hi.write(((lba >> 16) & 0xFF) as u8);
        p_cmd.write(0x30); // WRITE SECTORS (LBA28)
        retardo_400ns();
    }

    if !esperar_drq() {
        return false;
    }
    for i in 0..256 {
        let w = (buf[i * 2] as u16) | ((buf[i * 2 + 1] as u16) << 8);
        unsafe { p_data.write(w) };
    }
    if !esperar_no_ocupado() {
        return false;
    }
    // CACHE FLUSH para garantizar persistencia inmediata en disco real.
    unsafe {
        p_cmd.write(0xE7);
        retardo_400ns();
    }
    esperar_no_ocupado()
}

pub fn leer_sector_drive(drive: u8, lba: u32, buf: &mut [u8; 512]) -> bool {
    let _g = ATA_LOCK.lock();
    leer_sector_sin_lock(drive, lba, buf)
}

pub fn escribir_sector_drive(drive: u8, lba: u32, buf: &[u8; 512]) -> bool {
    let _g = ATA_LOCK.lock();
    escribir_sector_sin_lock(drive, lba, buf)
}

/// Lee bytes arbitrarios desde `/dev/sda` empezando en `offset` (solo lectura).
pub fn leer_bytes(offset: usize, dst: &mut [u8]) -> usize {
    let guard = DISCO_SDA.lock();
    let Some(info) = guard.as_ref() else {
        return 0;
    };
    let tam_total = (info.sectores as usize).saturating_mul(512);
    drop(guard);
    if offset >= tam_total || dst.is_empty() {
        return 0;
    }
    let limite = dst.len().min(tam_total - offset);
    let mut sector_buf = [0u8; 512];
    let mut leidos = 0usize;
    while leidos < limite {
        let pos = offset + leidos;
        let lba = (pos / 512) as u32;
        let dentro = pos % 512;
        if !leer_sector_drive(0, lba, &mut sector_buf) {
            break;
        }
        let cant = (512 - dentro).min(limite - leidos);
        dst[leidos..leidos + cant].copy_from_slice(&sector_buf[dentro..dentro + cant]);
        leidos += cant;
    }
    leidos
}

pub fn tamano_bytes() -> u64 {
    DISCO_SDA
        .lock()
        .as_ref()
        .map(|d| (d.sectores as u64) * 512)
        .unwrap_or(0)
}

pub fn formatear_proc_disk() -> String {
    let mut out = String::new();
    match DISCO_SDA.lock().as_ref() {
        Some(d) => {
            out.push_str(&format!(
                "/dev/sda (Boot): {} ({} KiB, MBR={})\n",
                d.modelo,
                (d.sectores as u64) / 2,
                if d.firma_mbr_ok { "OK" } else { "NO" }
            ));
        }
        None => out.push_str("/dev/sda: no detectado\n"),
    }
    match DISCO_SDB.lock().as_ref() {
        Some(d) => {
            let (archivos, usado_kb) = crate::neofs::estadisticas();
            out.push_str(&format!(
                "/dev/sdb (NeoFS RW): {} ({} MiB, {} archivos, {} KiB usados)\n",
                d.modelo,
                (d.sectores as u64) / 2048,
                archivos,
                usado_kb
            ));
        }
        None => out.push_str("/dev/sdb (NeoFS): sin disco persistente adjunto\n"),
    }
    out
}
