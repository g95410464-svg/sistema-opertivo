// Driver de graficos Bochs BGA / PCI Linear Framebuffer (800x600x32bpp) para `/dev/fb0`.
use spin::Mutex;
use x86_64::instructions::port::Port;

pub const ANCHO_FB: u32 = 1280;
pub const ALTO_FB: u32 = 720;
pub const BPP_FB: u32 = 32;
pub const BYTES_LINEA: u32 = ANCHO_FB * (BPP_FB / 8);
pub const TAMANO_FB: usize = (BYTES_LINEA * ALTO_FB) as usize;

const PCI_ADDR: u16 = 0x0CF8;
const PCI_DATA: u16 = 0x0CFC;

const VBE_INDEX: u16 = 0x01CE;
const VBE_DATA: u16 = 0x01CF;

const VBE_REG_ID: u16 = 0;
const VBE_REG_XRES: u16 = 1;
const VBE_REG_YRES: u16 = 2;
const VBE_REG_BPP: u16 = 3;
const VBE_REG_ENABLE: u16 = 4;
const VBE_REG_BANK: u16 = 5;
const VBE_REG_VIRT_WIDTH: u16 = 6;
const VBE_REG_VIRT_HEIGHT: u16 = 7;
const VBE_REG_X_OFFSET: u16 = 8;
const VBE_REG_Y_OFFSET: u16 = 9;

const VBE_DISABLED: u16 = 0x00;
const VBE_ENABLED: u16 = 0x01;
const VBE_LFB_ENABLED: u16 = 0x40;

#[derive(Clone, Copy)]
pub struct InfoFb {
    pub phys_addr: u64,
    pub ancho: u32,
    pub alto: u32,
    pub bpp: u32,
    pub bytes_linea: u32,
    pub tamano: usize,
    pub activo: bool,
}

static FB_ESTADO: Mutex<Option<InfoFb>> = Mutex::new(None);

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

unsafe fn vbe_escribir(reg: u16, valor: u16) {
    let mut p_idx: Port<u16> = Port::new(VBE_INDEX);
    let mut p_dat: Port<u16> = Port::new(VBE_DATA);
    p_idx.write(reg);
    p_dat.write(valor);
}

unsafe fn vbe_leer(reg: u16) -> u16 {
    let mut p_idx: Port<u16> = Port::new(VBE_INDEX);
    let mut p_dat: Port<u16> = Port::new(VBE_DATA);
    p_idx.write(reg);
    p_dat.read()
}

/// Escanea el bus PCI 0 buscando el adaptador Bochs/QEMU VGA (vendor 0x1234, device 0x1111)
/// o cualquier controlador VGA (clase 0x03) y lee su direccion fisica BAR0.
pub fn init() -> Option<InfoFb> {
    let vbe_id = unsafe { vbe_leer(VBE_REG_ID) };
    if (vbe_id & 0xFFF0) != 0xB0C0 {
        return None;
    }

    let mut bar0_encontrado: Option<u64> = None;
    for slot in 0..32u8 {
        let id = unsafe { pci_leer_u32(0, slot, 0, 0x00) };
        if id == 0xFFFF_FFFF {
            continue;
        }
        let vendor = (id & 0xFFFF) as u16;
        let device = (id >> 16) as u16;
        let clase_reg = unsafe { pci_leer_u32(0, slot, 0, 0x08) };
        let clase = (clase_reg >> 24) as u8;

        if (vendor == 0x1234 && device == 0x1111) || clase == 0x03 {
            let bar0 = unsafe { pci_leer_u32(0, slot, 0, 0x10) };
            let phys = (bar0 & 0xFFFF_FFF0) as u64;
            if phys != 0 {
                bar0_encontrado = Some(phys);
                if vendor == 0x1234 && device == 0x1111 {
                    break;
                }
            }
        }
    }

    let phys_addr = bar0_encontrado?;
    let info = InfoFb {
        phys_addr,
        ancho: ANCHO_FB,
        alto: ALTO_FB,
        bpp: BPP_FB,
        bytes_linea: BYTES_LINEA,
        tamano: TAMANO_FB,
        activo: false,
    };
    *FB_ESTADO.lock() = Some(info);
    Some(info)
}

pub fn info() -> Option<InfoFb> {
    *FB_ESTADO.lock()
}

/// Activa el modo grafico lineal 800x600x32bpp en el adaptador Bochs BGA.
pub fn activar_modo_grafico() -> Option<InfoFb> {
    let mut guard = FB_ESTADO.lock();
    let info = guard.as_mut()?;
    if !info.activo {
        unsafe {
            vbe_escribir(VBE_REG_ENABLE, VBE_DISABLED);
            vbe_escribir(VBE_REG_XRES, info.ancho as u16);
            vbe_escribir(VBE_REG_YRES, info.alto as u16);
            vbe_escribir(VBE_REG_BPP, info.bpp as u16);
            vbe_escribir(VBE_REG_BANK, 0);
            vbe_escribir(VBE_REG_VIRT_WIDTH, info.ancho as u16);
            vbe_escribir(VBE_REG_VIRT_HEIGHT, info.alto as u16);
            vbe_escribir(VBE_REG_X_OFFSET, 0);
            vbe_escribir(VBE_REG_Y_OFFSET, 0);
            vbe_escribir(VBE_REG_ENABLE, VBE_ENABLED | VBE_LFB_ENABLED);
        }
        info.activo = true;
    }
    Some(*info)
}

/// Regresa el adaptador a modo texto VGA (80x25).
#[allow(dead_code)]
pub fn desactivar_modo_grafico() {
    let mut guard = FB_ESTADO.lock();
    if let Some(info) = guard.as_mut() {
        if info.activo {
            unsafe {
                vbe_escribir(VBE_REG_ENABLE, VBE_DISABLED);
            }
            info.activo = false;
        }
    }
}
