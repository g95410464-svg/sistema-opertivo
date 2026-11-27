// NeoFS: sistema de archivos persistente real en bloques sobre `/dev/sdb` (`neo_disk.img`).
// Estructura en disco:
// - LBA 0: Superbloque (`NEOFS001`, total_sectores, proximo_sector_libre)
// - LBA 1..=32: Tabla de 256 inodos (64 bytes cada uno, 8 por sector)
// - LBA 33..N: Bloques de datos contiguos (512 bytes por sector)
use crate::disco;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

const DRIVE_PERSIST: u8 = 1;
const MAGIC: &[u8; 8] = b"NEOFS001";
const INODO_SECTORES: u32 = 32;
const MAX_INODOS: usize = (INODO_SECTORES as usize) * 8; // 256 archivos persistentes
const DATOS_INICIO: u32 = 1 + INODO_SECTORES; // LBA 33
const MAX_RUTA: usize = 48;
const MAX_ARCHIVO_BYTES: usize = 128 * 1024; // 128 KiB por archivo

#[derive(Clone, Copy)]
struct Inodo {
    activo: bool,
    ruta_len: u8,
    ruta: [u8; MAX_RUTA],
    lba_inicio: u32,
    capacidad_sectores: u32,
    tamano_bytes: u32,
}

impl Inodo {
    const fn vacio() -> Self {
        Inodo {
            activo: false,
            ruta_len: 0,
            ruta: [0; MAX_RUTA],
            lba_inicio: 0,
            capacidad_sectores: 0,
            tamano_bytes: 0,
        }
    }

    fn nombre_str(&self) -> &str {
        let n = (self.ruta_len as usize).min(MAX_RUTA);
        core::str::from_utf8(&self.ruta[..n]).unwrap_or("")
    }

    fn desde_bytes(b: &[u8]) -> Self {
        let mut r = [0u8; MAX_RUTA];
        r.copy_from_slice(&b[2..2 + MAX_RUTA]);
        Inodo {
            activo: b[0] == 1,
            ruta_len: b[1].min(MAX_RUTA as u8),
            ruta: r,
            lba_inicio: u32::from_le_bytes([b[52], b[53], b[54], b[55]]),
            capacidad_sectores: u32::from_le_bytes([b[56], b[57], b[58], b[59]]),
            tamano_bytes: u32::from_le_bytes([b[60], b[61], b[62], b[63]]),
        }
    }

    fn a_bytes(&self, dst: &mut [u8]) {
        dst.fill(0);
        dst[0] = if self.activo { 1 } else { 0 };
        dst[1] = self.ruta_len;
        dst[2..2 + MAX_RUTA].copy_from_slice(&self.ruta);
        dst[52..56].copy_from_slice(&self.lba_inicio.to_le_bytes());
        dst[56..60].copy_from_slice(&self.capacidad_sectores.to_le_bytes());
        dst[60..64].copy_from_slice(&self.tamano_bytes.to_le_bytes());
    }
}

struct EstadoNeoFs {
    montado: bool,
    recien_formateado: bool,
    total_sectores: u32,
    proximo_libre: u32,
    inodos: Vec<Inodo>,
}

static NEOFS: Mutex<EstadoNeoFs> = Mutex::new(EstadoNeoFs {
    montado: false,
    recien_formateado: false,
    total_sectores: 0,
    proximo_libre: DATOS_INICIO,
    inodos: Vec::new(),
});

pub fn es_ruta_persistente(ruta: &str) -> bool {
    ruta.starts_with("/home/guillermo/") || ruta.starts_with("/etc/neo_ws/")
}

fn escribir_superbloque(total_sectores: u32, proximo_libre: u32) -> bool {
    let mut sec = [0u8; 512];
    sec[0..8].copy_from_slice(MAGIC);
    sec[8..12].copy_from_slice(&total_sectores.to_le_bytes());
    sec[12..16].copy_from_slice(&proximo_libre.to_le_bytes());
    disco::escribir_sector_drive(DRIVE_PERSIST, 0, &sec)
}

fn escribir_sector_inodos(inodos: &[Inodo], sector_rel: usize) -> bool {
    let mut sec = [0u8; 512];
    let base = sector_rel * 8;
    for i in 0..8 {
        if base + i < inodos.len() {
            inodos[base + i].a_bytes(&mut sec[i * 64..(i + 1) * 64]);
        }
    }
    disco::escribir_sector_drive(DRIVE_PERSIST, 1 + sector_rel as u32, &sec)
}

/// Monta `/dev/sdb` (formateando automaticamente si es la primera vez).
/// Devuelve `Some(recien_formateado)` si hay disco persistente disponible.
pub fn init() -> Option<bool> {
    let info = disco::info_sdb()?;
    if info.sectores < DATOS_INICIO + 64 {
        return None;
    }

    let mut sb = [0u8; 512];
    if !disco::leer_sector_drive(DRIVE_PERSIST, 0, &mut sb) {
        return None;
    }

    let mut est = NEOFS.lock();
    est.inodos = alloc::vec![Inodo::vacio(); MAX_INODOS];
    est.total_sectores = info.sectores;

    if &sb[0..8] != MAGIC {
        // Primer arranque con disco nuevo: formateamos NeoFS limpiamente.
        est.proximo_libre = DATOS_INICIO;
        est.recien_formateado = true;
        if !escribir_superbloque(info.sectores, DATOS_INICIO) {
            return None;
        }
        let vacio = [0u8; 512];
        for s in 0..INODO_SECTORES {
            if !disco::escribir_sector_drive(DRIVE_PERSIST, 1 + s, &vacio) {
                return None;
            }
        }
        est.montado = true;
        return Some(true);
    }

    // Disco ya existente: cargamos superbloque y tabla de inodos desde `/dev/sdb`.
    let prox = u32::from_le_bytes([sb[12], sb[13], sb[14], sb[15]]);
    est.proximo_libre = prox.max(DATOS_INICIO).min(info.sectores);
    est.recien_formateado = false;

    let mut sec = [0u8; 512];
    for s in 0..(INODO_SECTORES as usize) {
        if disco::leer_sector_drive(DRIVE_PERSIST, 1 + s as u32, &mut sec) {
            for i in 0..8 {
                let inodo = Inodo::desde_bytes(&sec[i * 64..(i + 1) * 64]);
                est.inodos[s * 8 + i] = inodo;
            }
        }
    }
    est.montado = true;
    Some(false)
}

/// Lee todos los archivos almacenados en `/dev/sdb` para montarlos en el VFS al arrancar.
pub fn cargar_todos_los_archivos() -> Vec<(String, Vec<u8>)> {
    let est = NEOFS.lock();
    if !est.montado {
        return Vec::new();
    }
    let mut salida = Vec::new();
    let mut sec = [0u8; 512];

    for inodo in &est.inodos {
        if !inodo.activo || inodo.ruta_len == 0 {
            continue;
        }
        let ruta = String::from(inodo.nombre_str());
        let tam = (inodo.tamano_bytes as usize).min(MAX_ARCHIVO_BYTES);
        let mut datos = Vec::with_capacity(tam);
        let num_sec = (tam + 511) / 512;
        for s in 0..num_sec {
            let lba = inodo.lba_inicio + s as u32;
            if !disco::leer_sector_drive(DRIVE_PERSIST, lba, &mut sec) {
                break;
            }
            let faltan = tam - datos.len();
            let copiar = faltan.min(512);
            datos.extend_from_slice(&sec[..copiar]);
        }
        salida.push((ruta, datos));
    }
    salida
}

/// Guarda (crea o sobrescribe) un archivo de forma persistente en `/dev/sdb`.
pub fn guardar_archivo(ruta: &str, datos: &[u8]) -> bool {
    if !es_ruta_persistente(ruta) || ruta.len() > MAX_RUTA || datos.len() > MAX_ARCHIVO_BYTES {
        return false;
    }
    let mut est = NEOFS.lock();
    if !est.montado {
        return false;
    }

    let sec_necesarios = ((datos.len() + 511) / 512).max(1) as u32;

    // 1) Buscar inodo existente o ranura libre.
    let mut idx_encontrado = None;
    let mut idx_libre = None;
    for (i, inodo) in est.inodos.iter().enumerate() {
        if inodo.activo && inodo.nombre_str() == ruta {
            idx_encontrado = Some(i);
            break;
        }
        if !inodo.activo && idx_libre.is_none() {
            idx_libre = Some(i);
        }
    }

    let idx = match idx_encontrado.or(idx_libre) {
        Some(i) => i,
        None => return false,
    };

    // 2) Reutilizar bloques si caben; si crecio, asignar bloques nuevos al final.
    let mut inodo = est.inodos[idx];
    let mut superbloque_cambio = false;
    if !inodo.activo || inodo.capacidad_sectores < sec_necesarios {
        let cap = sec_necesarios.max(8); // Reservamos minimo 4 KiB por archivo para ediciones futuras
        if est.proximo_libre.saturating_add(cap) > est.total_sectores {
            return false;
        }
        inodo.lba_inicio = est.proximo_libre;
        inodo.capacidad_sectores = cap;
        est.proximo_libre += cap;
        superbloque_cambio = true;
    }

    inodo.activo = true;
    inodo.ruta_len = ruta.len() as u8;
    inodo.ruta = [0; MAX_RUTA];
    inodo.ruta[..ruta.len()].copy_from_slice(ruta.as_bytes());
    inodo.tamano_bytes = datos.len() as u32;

    // 3) Escribir sectores de datos.
    let mut sec = [0u8; 512];
    for s in 0..(sec_necesarios as usize) {
        sec.fill(0);
        let ini = s * 512;
        if ini < datos.len() {
            let fin = (ini + 512).min(datos.len());
            sec[..fin - ini].copy_from_slice(&datos[ini..fin]);
        }
        if !disco::escribir_sector_drive(DRIVE_PERSIST, inodo.lba_inicio + s as u32, &sec) {
            return false;
        }
    }

    // 4) Actualizar tabla de inodos y superbloque.
    est.inodos[idx] = inodo;
    if !escribir_sector_inodos(&est.inodos, idx / 8) {
        return false;
    }
    if superbloque_cambio {
        let _ = escribir_superbloque(est.total_sectores, est.proximo_libre);
    }
    true
}

#[allow(dead_code)]
pub fn esta_montado() -> bool {
    NEOFS.lock().montado
}

pub fn estadisticas() -> (usize, u64) {
    let est = NEOFS.lock();
    if !est.montado {
        return (0, 0);
    }
    let mut archivos = 0usize;
    let mut bytes = 0u64;
    for inodo in &est.inodos {
        if inodo.activo {
            archivos += 1;
            bytes += inodo.tamano_bytes as u64;
        }
    }
    (archivos, (bytes + 1023) / 1024)
}
