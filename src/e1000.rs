// Driver PCI de tarjeta de red Intel e1000 (82540EM, vendor 0x8086, device 0x100E).
// Usa anillos DMA fisicos en memoria baja + MMIO mapeado en Ring 0 para TX y RX reales.
use crate::memoria;
use core::sync::atomic::{fence, Ordering};
use spin::Mutex;
use x86_64::instructions::port::Port;

const PCI_ADDR: u16 = 0x0CF8;
const PCI_DATA: u16 = 0x0CFC;

// Registros MMIO del controlador Intel e1000
const REG_CTRL: usize = 0x0000;
const REG_EERD: usize = 0x0014;
const REG_IMC: usize = 0x00D8;
const REG_RCTL: usize = 0x0100;
const REG_TCTL: usize = 0x0400;
const REG_TIPG: usize = 0x0410;
const REG_RDBAL: usize = 0x2800;
const REG_RDBAH: usize = 0x2804;
const REG_RDLEN: usize = 0x2808;
const REG_RDH: usize = 0x2810;
const REG_RDT: usize = 0x2818;
const REG_TDBAL: usize = 0x3800;
const REG_TDBAH: usize = 0x3804;
const REG_TDLEN: usize = 0x3808;
const REG_TDH: usize = 0x3810;
const REG_TDT: usize = 0x3818;
const REG_MTA: usize = 0x5200;
const REG_RAL0: usize = 0x5400;
const REG_RAH0: usize = 0x5404;

const NUM_RX: usize = 32;
const NUM_TX: usize = 16;
const BUF_TAM: usize = 2048;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct RxDesc {
    addr: u64,
    length: u16,
    checksum: u16,
    status: u8,
    errors: u8,
    special: u16,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct TxDesc {
    addr: u64,
    length: u16,
    cso: u8,
    cmd: u8,
    status: u8,
    css: u8,
    special: u16,
}

struct EstadoE1000 {
    mmio: u64,
    mac: [u8; 6],
    rx_ring: *mut RxDesc,
    rx_bufs: [*mut u8; NUM_RX],
    rx_cur: usize,
    tx_ring: *mut TxDesc,
    tx_bufs: [*mut u8; NUM_TX],
    tx_cur: usize,
    tx_pkts: u64,
    rx_pkts: u64,
}

unsafe impl Send for EstadoE1000 {}

static E1000: Mutex<Option<EstadoE1000>> = Mutex::new(None);

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

#[inline(always)]
fn leer_reg(mmio: u64, reg: usize) -> u32 {
    unsafe { core::ptr::read_volatile((mmio as usize + reg) as *const u32) }
}

#[inline(always)]
fn escribir_reg(mmio: u64, reg: usize, val: u32) {
    unsafe { core::ptr::write_volatile((mmio as usize + reg) as *mut u32, val) }
}

fn leer_eeprom(mmio: u64, addr: u32) -> Option<u16> {
    escribir_reg(mmio, REG_EERD, 1 | (addr << 8));
    for _ in 0..10_000 {
        let v = leer_reg(mmio, REG_EERD);
        if (v & (1 << 4)) != 0 {
            return Some((v >> 16) as u16);
        }
    }
    None
}

fn leer_mac(mmio: u64) -> [u8; 6] {
    let ral = leer_reg(mmio, REG_RAL0);
    let rah = leer_reg(mmio, REG_RAH0);
    if ral != 0 || (rah & 0xFFFF) != 0 {
        return [
            (ral & 0xFF) as u8,
            ((ral >> 8) & 0xFF) as u8,
            ((ral >> 16) & 0xFF) as u8,
            ((ral >> 24) & 0xFF) as u8,
            (rah & 0xFF) as u8,
            ((rah >> 8) & 0xFF) as u8,
        ];
    }
    if let (Some(w0), Some(w1), Some(w2)) = (
        leer_eeprom(mmio, 0),
        leer_eeprom(mmio, 1),
        leer_eeprom(mmio, 2),
    ) {
        return [
            (w0 & 0xFF) as u8,
            (w0 >> 8) as u8,
            (w1 & 0xFF) as u8,
            (w1 >> 8) as u8,
            (w2 & 0xFF) as u8,
            (w2 >> 8) as u8,
        ];
    }
    [0x52, 0x54, 0x00, 0x12, 0x34, 0x56]
}

/// Detecta e inicializa el adaptador Intel e1000 en PCI bus 0. Devuelve la MAC si esta activo.
pub fn init() -> Option<[u8; 6]> {
    let mut encontrado: Option<(u8, u64)> = None;
    for slot in 0..32u8 {
        let id = unsafe { pci_leer_u32(0, slot, 0, 0x00) };
        if id == 0xFFFF_FFFF {
            continue;
        }
        let vendor = (id & 0xFFFF) as u16;
        let device = (id >> 16) as u16;
        if vendor == 0x8086 && matches!(device, 0x100E | 0x100F | 0x10D3 | 0x1004 | 0x1019) {
            let bar0 = unsafe { pci_leer_u32(0, slot, 0, 0x10) };
            let phys = (bar0 & 0xFFFF_FFF0) as u64;
            if phys != 0 {
                // Activar Bus Master (bit 2) y Memory Space (bit 1) en PCI Command
                let cmd = unsafe { pci_leer_u32(0, slot, 0, 0x04) };
                unsafe { pci_escribir_u32(0, slot, 0, 0x04, cmd | 0x06) };
                encontrado = Some((slot, phys));
                break;
            }
        }
    }

    let (_, bar0_phys) = encontrado?;
    let mmio = memoria::mapear_mmio_kernel(bar0_phys, 128 * 1024)?;

    // Reset suave + activar enlace (SLU) + enmascarar interrupciones (modo polling limpio)
    escribir_reg(mmio, REG_IMC, 0xFFFF_FFFF);
    let ctrl = leer_reg(mmio, REG_CTRL);
    escribir_reg(mmio, REG_CTRL, (ctrl | (1 << 6)) & !(1 << 3) & !(1 << 31));
    for i in 0..128 {
        escribir_reg(mmio, REG_MTA + i * 4, 0);
    }

    let mac = leer_mac(mmio);

    // 1) Anillo RX (32 descriptores en 1 frame DMA + 16 frames para 32 buffers de 2 KiB)
    let (rx_ring_phys, rx_ring_ptr) = memoria::asignar_frame_dma()?;
    let rx_ring = rx_ring_ptr as *mut RxDesc;
    let mut rx_bufs = [core::ptr::null_mut::<u8>(); NUM_RX];

    for i in 0..(NUM_RX / 2) {
        let (phys, ptr) = memoria::asignar_frame_dma()?;
        for mitad in 0..2 {
            let idx = i * 2 + mitad;
            let off = mitad * BUF_TAM;
            rx_bufs[idx] = unsafe { ptr.add(off) };
            unsafe {
                (*rx_ring.add(idx)) = RxDesc {
                    addr: phys + off as u64,
                    length: 0,
                    checksum: 0,
                    status: 0,
                    errors: 0,
                    special: 0,
                };
            }
        }
    }

    escribir_reg(mmio, REG_RDBAL, (rx_ring_phys & 0xFFFF_FFFF) as u32);
    escribir_reg(mmio, REG_RDBAH, (rx_ring_phys >> 32) as u32);
    escribir_reg(mmio, REG_RDLEN, (NUM_RX * core::mem::size_of::<RxDesc>()) as u32);
    escribir_reg(mmio, REG_RDH, 0);
    escribir_reg(mmio, REG_RDT, (NUM_RX - 1) as u32);
    // RCTL: EN | SBP | UPE | MPE | BAM | SECRC (buffers 2048 B)
    escribir_reg(
        mmio,
        REG_RCTL,
        (1 << 1) | (1 << 2) | (1 << 3) | (1 << 4) | (1 << 15) | (1 << 26),
    );

    // 2) Anillo TX (16 descriptores en 1 frame DMA + 8 frames para 16 buffers de 2 KiB)
    let (tx_ring_phys, tx_ring_ptr) = memoria::asignar_frame_dma()?;
    let tx_ring = tx_ring_ptr as *mut TxDesc;
    let mut tx_bufs = [core::ptr::null_mut::<u8>(); NUM_TX];

    for i in 0..(NUM_TX / 2) {
        let (phys, ptr) = memoria::asignar_frame_dma()?;
        for mitad in 0..2 {
            let idx = i * 2 + mitad;
            let off = mitad * BUF_TAM;
            tx_bufs[idx] = unsafe { ptr.add(off) };
            unsafe {
                (*tx_ring.add(idx)) = TxDesc {
                    addr: phys + off as u64,
                    length: 0,
                    cso: 0,
                    cmd: 0,
                    status: 1, // DD=1 listo para usar
                    css: 0,
                    special: 0,
                };
            }
        }
    }

    escribir_reg(mmio, REG_TDBAL, (tx_ring_phys & 0xFFFF_FFFF) as u32);
    escribir_reg(mmio, REG_TDBAH, (tx_ring_phys >> 32) as u32);
    escribir_reg(mmio, REG_TDLEN, (NUM_TX * core::mem::size_of::<TxDesc>()) as u32);
    escribir_reg(mmio, REG_TDH, 0);
    escribir_reg(mmio, REG_TDT, 0);
    // TCTL: EN | PSP | CT=15 | COLD=64
    escribir_reg(mmio, REG_TCTL, (1 << 1) | (1 << 3) | (15 << 4) | (64 << 12));
    escribir_reg(mmio, REG_TIPG, 0x0060_200A);

    *E1000.lock() = Some(EstadoE1000 {
        mmio,
        mac,
        rx_ring,
        rx_bufs,
        rx_cur: 0,
        tx_ring,
        tx_bufs,
        tx_cur: 0,
        tx_pkts: 0,
        rx_pkts: 0,
    });
    Some(mac)
}

pub fn mac() -> Option<[u8; 6]> {
    E1000.lock().as_ref().map(|e| e.mac)
}

pub fn estadisticas() -> Option<([u8; 6], u64, u64)> {
    E1000.lock().as_ref().map(|e| (e.mac, e.tx_pkts, e.rx_pkts))
}

/// Envia una trama Ethernet cruda (max 1518 bytes) por la tarjeta e1000.
pub fn enviar_trama(datos: &[u8]) -> bool {
    if datos.is_empty() || datos.len() > 1518 {
        return false;
    }
    let mut guard = E1000.lock();
    let Some(e) = guard.as_mut() else {
        return false;
    };
    let idx = e.tx_cur;
    unsafe {
        core::ptr::copy_nonoverlapping(datos.as_ptr(), e.tx_bufs[idx], datos.len());
        let d = &mut *e.tx_ring.add(idx);
        d.length = datos.len() as u16;
        d.cmd = (1 << 0) | (1 << 1) | (1 << 3); // EOP | IFCS | RS
        d.status = 0;
    }
    fence(Ordering::SeqCst);
    let sig = (idx + 1) % NUM_TX;
    e.tx_cur = sig;
    escribir_reg(e.mmio, REG_TDT, sig as u32);

    // Esperar bit DD (Descriptor Done)
    for _ in 0..200_000 {
        let desc = unsafe { core::ptr::read_volatile(e.tx_ring.add(idx)) };
        if (desc.status & 1) != 0 {
            e.tx_pkts += 1;
            return true;
        }
        core::hint::spin_loop();
    }
    e.tx_pkts += 1;
    true
}

/// Recibe una trama Ethernet si hay una lista en el anillo RX (no bloqueante).
pub fn recibir_trama(dst: &mut [u8]) -> Option<usize> {
    let mut guard = E1000.lock();
    let e = guard.as_mut()?;
    let idx = e.rx_cur;
    fence(Ordering::SeqCst);
    let desc = unsafe { core::ptr::read_volatile(e.rx_ring.add(idx)) };
    if (desc.status & 1) == 0 {
        return None;
    }
    let len = desc.length as usize;
    let copiar = len.min(dst.len()).min(BUF_TAM);
    unsafe {
        core::ptr::copy_nonoverlapping(e.rx_bufs[idx], dst.as_mut_ptr(), copiar);
        let mut limpio = desc;
        limpio.status = 0;
        core::ptr::write_volatile(e.rx_ring.add(idx), limpio);
    }
    let viejo = idx;
    e.rx_cur = (idx + 1) % NUM_RX;
    escribir_reg(e.mmio, REG_RDT, viejo as u32);
    e.rx_pkts += 1;
    Some(copiar)
}
