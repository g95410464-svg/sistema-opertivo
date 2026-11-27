// Sistema de archivos en memoria (ramfs) y tabla de descriptores de archivo (FD).
use crate::interrupciones;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

// Binarios ELF de Linux embebidos dentro de /bin del ramfs inicial.
static ELF_HOLA: &[u8] = include_bytes!("../programas/hola_linux.elf");
static ELF_SH: &[u8] = include_bytes!("../programas/sh.elf");

// Flags de open/openat de Linux
const O_WRONLY: u64 = 1;
const O_RDWR: u64 = 2;
const O_CREAT: u64 = 0x40;
const O_TRUNC: u64 = 0x200;
const O_APPEND: u64 = 0x400;

// Codigos de error Linux (negativos)
const ENOENT: i64 = -2;
const EBADF: i64 = -9;
const EACCES: i64 = -13;
const ENOTDIR: i64 = -20;
const EISDIR: i64 = -21;
const EMFILE: i64 = -24;

// Tipos de entrada para linux_dirent64
const DT_CHR: u8 = 2;
const DT_DIR: u8 = 4;
const DT_REG: u8 = 8;

enum Nodo {
    Directorio,
    Archivo(Vec<u8>),
    DevNull,
    DevZero,
    ProcUptime,
}

struct DescriptorAbierto {
    ruta: String,
    posicion: usize,
    puede_leer: bool,
    puede_escribir: bool,
    es_directorio: bool,
}

struct EstadoFs {
    nodos: BTreeMap<String, Nodo>,
    fds: [Option<DescriptorAbierto>; 32],
}

static FS: Mutex<Option<EstadoFs>> = Mutex::new(None);

pub fn init() {
    let mut nodos = BTreeMap::new();
    nodos.insert("/".to_string(), Nodo::Directorio);
    nodos.insert("/bin".to_string(), Nodo::Directorio);
    nodos.insert("/etc".to_string(), Nodo::Directorio);
    nodos.insert("/dev".to_string(), Nodo::Directorio);
    nodos.insert("/proc".to_string(), Nodo::Directorio);
    nodos.insert("/tmp".to_string(), Nodo::Directorio);

    nodos.insert("/bin/hola".to_string(), Nodo::Archivo(ELF_HOLA.to_vec()));
    nodos.insert("/bin/sh".to_string(), Nodo::Archivo(ELF_SH.to_vec()));
    nodos.insert(
        "/etc/hostname".to_string(),
        Nodo::Archivo(b"neo_ws\n".to_vec()),
    );
    nodos.insert(
        "/etc/os-release".to_string(),
        Nodo::Archivo(
            b"NAME=\"neo_ws\"\nVERSION=\"0.1\"\nID=neo_ws\nPRETTY_NAME=\"neo_ws 0.1 (Linux ABI)\"\n"
                .to_vec(),
        ),
    );
    nodos.insert(
        "/etc/motd".to_string(),
        Nodo::Archivo(
            b"=== neo_ws 0.1 (Ring 3 / Linux x86_64 ABI) ===\nEscribe 'help' para ver los comandos.\n"
                .to_vec(),
        ),
    );
    nodos.insert("/dev/null".to_string(), Nodo::DevNull);
    nodos.insert("/dev/zero".to_string(), Nodo::DevZero);
    nodos.insert("/proc/uptime".to_string(), Nodo::ProcUptime);

    const NINGUNO: Option<DescriptorAbierto> = None;
    *FS.lock() = Some(EstadoFs {
        nodos,
        fds: [NINGUNO; 32],
    });
}

fn normalizar_ruta(ruta: &str) -> String {
    let mut partes: Vec<&str> = Vec::new();
    for p in ruta.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                partes.pop();
            }
            otra => partes.push(otra),
        }
    }
    if partes.is_empty() {
        return "/".to_string();
    }
    let mut res = String::new();
    for p in partes {
        res.push('/');
        res.push_str(p);
    }
    res
}

fn directorio_padre(ruta: &str) -> String {
    if ruta == "/" {
        return "/".to_string();
    }
    match ruta.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(i) => ruta[..i].to_string(),
    }
}

/// Lee un archivo completo desde el kernel (por ejemplo, para cargar `/bin/sh`).
pub fn leer_archivo_kernel(ruta: &str) -> Option<Vec<u8>> {
    let norm = normalizar_ruta(ruta);
    let guard = FS.lock();
    let fs = guard.as_ref()?;
    match fs.nodos.get(&norm) {
        Some(Nodo::Archivo(datos)) => Some(datos.clone()),
        _ => None,
    }
}

pub fn abrir(ruta: &str, flags: u64) -> i64 {
    let norm = normalizar_ruta(ruta);
    let modo = flags & 3;
    let puede_leer = modo == 0 || modo == O_RDWR;
    let puede_escribir = modo == O_WRONLY || modo == O_RDWR;

    let mut guard = FS.lock();
    let Some(fs) = guard.as_mut() else {
        return ENOENT;
    };

    if !fs.nodos.contains_key(&norm) {
        if (flags & O_CREAT) == 0 {
            return ENOENT;
        }
        let padre = directorio_padre(&norm);
        match fs.nodos.get(&padre) {
            Some(Nodo::Directorio) => {
                fs.nodos.insert(norm.clone(), Nodo::Archivo(Vec::new()));
            }
            _ => return ENOENT,
        }
    }

    let (es_directorio, posicion) = match fs.nodos.get_mut(&norm) {
        Some(Nodo::Directorio) => {
            if puede_escribir {
                return EISDIR;
            }
            (true, 0)
        }
        Some(Nodo::Archivo(datos)) => {
            if puede_escribir && (flags & O_TRUNC) != 0 {
                datos.clear();
            }
            let pos = if (flags & O_APPEND) != 0 {
                datos.len()
            } else {
                0
            };
            (false, pos)
        }
        Some(Nodo::DevNull | Nodo::DevZero | Nodo::ProcUptime) => (false, 0),
        None => return ENOENT,
    };

    // Buscamos el primer FD libre a partir de 3 (0=stdin, 1=stdout, 2=stderr).
    for fd in 3..fs.fds.len() {
        if fs.fds[fd].is_none() {
            fs.fds[fd] = Some(DescriptorAbierto {
                ruta: norm,
                posicion,
                puede_leer,
                puede_escribir,
                es_directorio,
            });
            return fd as i64;
        }
    }
    EMFILE
}

pub fn cerrar(fd: usize) -> i64 {
    if fd < 3 {
        return 0;
    }
    let mut guard = FS.lock();
    let Some(fs) = guard.as_mut() else {
        return EBADF;
    };
    if fd >= fs.fds.len() || fs.fds[fd].is_none() {
        return EBADF;
    }
    fs.fds[fd] = None;
    0
}

pub fn leer(fd: usize, dst: &mut [u8]) -> i64 {
    let mut guard = FS.lock();
    let Some(fs) = guard.as_mut() else {
        return EBADF;
    };
    let Some(desc) = fs.fds.get_mut(fd).and_then(|d| d.as_mut()) else {
        return EBADF;
    };
    if !desc.puede_leer {
        return EACCES;
    }
    if desc.es_directorio {
        return EISDIR;
    }

    let ruta = desc.ruta.clone();
    let pos = desc.posicion;

    let leidos = match fs.nodos.get(&ruta) {
        Some(Nodo::Archivo(datos)) => {
            if pos >= datos.len() {
                0
            } else {
                let cant = dst.len().min(datos.len() - pos);
                dst[..cant].copy_from_slice(&datos[pos..pos + cant]);
                cant
            }
        }
        Some(Nodo::DevNull) => 0,
        Some(Nodo::DevZero) => {
            dst.fill(0);
            dst.len()
        }
        Some(Nodo::ProcUptime) => {
            let t = interrupciones::ticks();
            // El timer PIT dispara ~18.2 veces por segundo (55 ms por tick).
            let ms = t * 55;
            let texto = format!("{}.{:02} s ({} ticks)\n", ms / 1000, (ms % 1000) / 10, t);
            let b = texto.as_bytes();
            if pos >= b.len() {
                0
            } else {
                let cant = dst.len().min(b.len() - pos);
                dst[..cant].copy_from_slice(&b[pos..pos + cant]);
                cant
            }
        }
        _ => return EBADF,
    };

    if let Some(desc) = fs.fds[fd].as_mut() {
        desc.posicion += leidos;
    }
    leidos as i64
}

pub fn escribir(fd: usize, src: &[u8]) -> i64 {
    let mut guard = FS.lock();
    let Some(fs) = guard.as_mut() else {
        return EBADF;
    };
    let Some(desc) = fs.fds.get_mut(fd).and_then(|d| d.as_mut()) else {
        return EBADF;
    };
    if !desc.puede_escribir {
        return EACCES;
    }
    if desc.es_directorio {
        return EISDIR;
    }

    let ruta = desc.ruta.clone();
    let pos = desc.posicion;

    match fs.nodos.get_mut(&ruta) {
        Some(Nodo::Archivo(datos)) => {
            if pos > datos.len() {
                datos.resize(pos, 0);
            }
            if pos + src.len() > datos.len() {
                datos.resize(pos + src.len(), 0);
            }
            datos[pos..pos + src.len()].copy_from_slice(src);
        }
        Some(Nodo::DevNull | Nodo::DevZero) => {}
        _ => return EBADF,
    }

    if let Some(desc) = fs.fds[fd].as_mut() {
        desc.posicion += src.len();
    }
    src.len() as i64
}

/// Implementacion de `getdents64` de Linux: llena `dst` con registros `linux_dirent64`.
pub fn getdents64(fd: usize, dst: &mut [u8]) -> i64 {
    let mut guard = FS.lock();
    let Some(fs) = guard.as_mut() else {
        return EBADF;
    };
    let Some(desc) = fs.fds.get(fd).and_then(|d| d.as_ref()) else {
        return EBADF;
    };
    if !desc.es_directorio {
        return ENOTDIR;
    }

    let dir = desc.ruta.clone();
    let inicio_idx = desc.posicion;

    // Recolectamos hijos directos del directorio `dir`.
    let mut hijos: Vec<(String, u8)> = Vec::new();
    for (ruta, nodo) in &fs.nodos {
        if ruta == &dir {
            continue;
        }
        if directorio_padre(ruta) == dir {
            let nombre = ruta.rsplit('/').next().unwrap_or("");
            let dtype = match nodo {
                Nodo::Directorio => DT_DIR,
                Nodo::Archivo(_) | Nodo::ProcUptime => DT_REG,
                Nodo::DevNull | Nodo::DevZero => DT_CHR,
            };
            hijos.push((nombre.to_string(), dtype));
        }
    }

    let mut escrito = 0usize;
    let mut nuevo_idx = inicio_idx;

    for (idx, (nombre, dtype)) in hijos.iter().enumerate().skip(inicio_idx) {
        let nb = nombre.as_bytes();
        // linux_dirent64: d_ino(8) + d_off(8) + d_reclen(2) + d_type(1) + name + '\0', alineado a 8.
        let reclen = (19 + nb.len() + 1 + 7) & !7;
        if escrito + reclen > dst.len() {
            break;
        }
        let ino = (idx as u64) + 2;
        let off = (idx as i64) + 1;
        dst[escrito..escrito + 8].copy_from_slice(&ino.to_le_bytes());
        dst[escrito + 8..escrito + 16].copy_from_slice(&off.to_le_bytes());
        dst[escrito + 16..escrito + 18].copy_from_slice(&(reclen as u16).to_le_bytes());
        dst[escrito + 18] = *dtype;
        dst[escrito + 19..escrito + 19 + nb.len()].copy_from_slice(nb);
        for b in &mut dst[escrito + 19 + nb.len()..escrito + reclen] {
            *b = 0;
        }
        escrito += reclen;
        nuevo_idx = idx + 1;
    }

    if let Some(desc) = fs.fds[fd].as_mut() {
        desc.posicion = nuevo_idx;
    }
    escrito as i64
}
