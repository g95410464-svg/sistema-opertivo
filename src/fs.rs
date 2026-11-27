// Sistema de archivos en memoria (ramfs), pipes Unix, stat y tabla de FDs por proceso.
use crate::{audio, disco, fb, interrupciones, memoria, neofs, net, proceso, reloj};
use alloc::collections::{BTreeMap, VecDeque};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

// Binarios ELF de Linux y recursos embebidos en el ramfs inicial (cero copia en el heap).
static ELF_HOLA: &[u8] = include_bytes!("../programas/hola_linux.elf");
static ELF_HOLA_STD: &[u8] = include_bytes!("../programas/hola_std.elf");
static ELF_FORKTEST: &[u8] = include_bytes!("../programas/forktest.elf");
static ELF_UPPER: &[u8] = include_bytes!("../programas/upper.elf");
static ELF_SH: &[u8] = include_bytes!("../programas/sh.elf");
static ELF_DESKTOP: &[u8] = include_bytes!("../programas/desktop.elf");
static WALLPAPER_RGB565: &[u8] = include_bytes!("../programas/wallpaper.rgb565");

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
const EINVAL: i64 = -22;
const EMFILE: i64 = -24;
const ESPIPE: i64 = -29;
const EPIPE: i64 = -32;

// Tipos de entrada para linux_dirent64
const DT_CHR: u8 = 2;
const DT_DIR: u8 = 4;
const DT_REG: u8 = 8;

// Bits de st_mode para struct stat de Linux x86_64
const S_IFIFO: u32 = 0o010000;
const S_IFCHR: u32 = 0o020000;
const S_IFDIR: u32 = 0o040000;
const S_IFREG: u32 = 0o100000;

enum Nodo {
    Directorio,
    Archivo(Vec<u8>),
    ArchivoEstatico(&'static [u8]),
    DevNull,
    DevZero,
    DevFb0,
    DevInputEvents,
    DevSda,
    DevNet,
    DevDsp,
    ProcUptime,
    ProcPs,
    ProcDisk,
    ProcMeminfo,
    ProcTime,
    ProcNet,
}

pub struct BufferPipe {
    pub datos: VecDeque<u8>,
    pub escritores: usize,
    pub lectores: usize,
}

pub struct PipeLectura(pub Arc<Mutex<BufferPipe>>);
pub struct PipeEscritura(pub Arc<Mutex<BufferPipe>>);

impl Clone for PipeLectura {
    fn clone(&self) -> Self {
        self.0.lock().lectores += 1;
        PipeLectura(self.0.clone())
    }
}

impl Drop for PipeLectura {
    fn drop(&mut self) {
        let mut p = self.0.lock();
        p.lectores = p.lectores.saturating_sub(1);
    }
}

impl Clone for PipeEscritura {
    fn clone(&self) -> Self {
        self.0.lock().escritores += 1;
        PipeEscritura(self.0.clone())
    }
}

impl Drop for PipeEscritura {
    fn drop(&mut self) {
        let mut p = self.0.lock();
        p.escritores = p.escritores.saturating_sub(1);
    }
}

pub fn crear_pipe() -> (Descriptor, Descriptor) {
    let buf = Arc::new(Mutex::new(BufferPipe {
        datos: VecDeque::with_capacity(256),
        escritores: 1,
        lectores: 1,
    }));
    (
        Descriptor::PipeLectura(PipeLectura(buf.clone())),
        Descriptor::PipeEscritura(PipeEscritura(buf)),
    )
}

pub struct EstadoArchivo {
    pub ruta: String,
    pub posicion: usize,
    pub puede_leer: bool,
    pub puede_escribir: bool,
    pub es_directorio: bool,
}

#[derive(Clone)]
pub enum Descriptor {
    TtyEntrada,
    TtySalida,
    DevFb0,
    DevInputEvents,
    Archivo(Arc<Mutex<EstadoArchivo>>),
    PipeLectura(PipeLectura),
    PipeEscritura(PipeEscritura),
}

#[derive(Clone)]
pub struct TablaFds {
    pub fds: [Option<Descriptor>; 32],
}

impl TablaFds {
    pub fn nueva() -> Self {
        const NINGUNO: Option<Descriptor> = None;
        let mut fds = [NINGUNO; 32];
        fds[0] = Some(Descriptor::TtyEntrada);
        fds[1] = Some(Descriptor::TtySalida);
        fds[2] = Some(Descriptor::TtySalida);
        TablaFds { fds }
    }

    pub fn insertar(&mut self, desc: Descriptor) -> i64 {
        for fd in 0..self.fds.len() {
            if self.fds[fd].is_none() {
                self.fds[fd] = Some(desc);
                return fd as i64;
            }
        }
        EMFILE
    }

    pub fn insertar_desde(&mut self, minimo: usize, desc: Descriptor) -> i64 {
        for fd in minimo..self.fds.len() {
            if self.fds[fd].is_none() {
                self.fds[fd] = Some(desc);
                return fd as i64;
            }
        }
        EMFILE
    }

    pub fn cerrar(&mut self, fd: usize) -> i64 {
        if fd >= self.fds.len() || self.fds[fd].is_none() {
            return EBADF;
        }
        self.fds[fd] = None;
        0
    }

    pub fn dup(&mut self, viejo: usize) -> i64 {
        let Some(desc) = self.fds.get(viejo).and_then(|d| d.clone()) else {
            return EBADF;
        };
        self.insertar(desc)
    }

    pub fn dup2(&mut self, viejo: usize, nuevo: usize) -> i64 {
        if viejo >= self.fds.len() || nuevo >= self.fds.len() {
            return EBADF;
        }
        let Some(desc) = self.fds[viejo].clone() else {
            return EBADF;
        };
        if viejo == nuevo {
            return nuevo as i64;
        }
        self.fds[nuevo] = Some(desc);
        nuevo as i64
    }
}

/// Struct `stat` exacto de Linux x86_64 (144 bytes).
#[repr(C)]
#[derive(Default)]
pub struct StatLinux {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_nlink: u64,
    pub st_mode: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub _pad0: u32,
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_blksize: i64,
    pub st_blocks: i64,
    pub st_atime: i64,
    pub st_atime_nsec: i64,
    pub st_mtime: i64,
    pub st_mtime_nsec: i64,
    pub st_ctime: i64,
    pub st_ctime_nsec: i64,
    pub _unused: [i64; 3],
}

static NODOS: Mutex<Option<BTreeMap<String, Nodo>>> = Mutex::new(None);

pub fn init() {
    let mut nodos = BTreeMap::new();
    nodos.insert("/".to_string(), Nodo::Directorio);
    nodos.insert("/bin".to_string(), Nodo::Directorio);
    nodos.insert("/etc".to_string(), Nodo::Directorio);
    nodos.insert("/etc/neo_ws".to_string(), Nodo::Directorio);
    nodos.insert("/dev".to_string(), Nodo::Directorio);
    nodos.insert("/dev/input".to_string(), Nodo::Directorio);
    nodos.insert("/proc".to_string(), Nodo::Directorio);
    nodos.insert("/tmp".to_string(), Nodo::Directorio);
    nodos.insert("/home".to_string(), Nodo::Directorio);
    nodos.insert("/home/guillermo".to_string(), Nodo::Directorio);
    nodos.insert("/usr".to_string(), Nodo::Directorio);
    nodos.insert("/usr/share".to_string(), Nodo::Directorio);

    nodos.insert("/bin/hola".to_string(), Nodo::ArchivoEstatico(ELF_HOLA));
    nodos.insert(
        "/bin/hola_std".to_string(),
        Nodo::ArchivoEstatico(ELF_HOLA_STD),
    );
    nodos.insert(
        "/bin/forktest".to_string(),
        Nodo::ArchivoEstatico(ELF_FORKTEST),
    );
    nodos.insert("/bin/upper".to_string(), Nodo::ArchivoEstatico(ELF_UPPER));
    nodos.insert("/bin/sh".to_string(), Nodo::ArchivoEstatico(ELF_SH));
    nodos.insert(
        "/bin/desktop".to_string(),
        Nodo::ArchivoEstatico(ELF_DESKTOP),
    );
    nodos.insert(
        "/usr/share/wallpaper.rgb".to_string(),
        Nodo::ArchivoEstatico(WALLPAPER_RGB565),
    );

    nodos.insert(
        "/etc/hostname".to_string(),
        Nodo::Archivo(b"neo_ws\n".to_vec()),
    );
    nodos.insert(
        "/etc/passwd".to_string(),
        Nodo::Archivo(b"guillermo:neo:1000:1000:Guillermo:/home/guillermo:/bin/sh\n".to_vec()),
    );
    nodos.insert(
        "/home/guillermo/ejemplo.py".to_string(),
        Nodo::Archivo(
            b"# Ejemplo en /home/guillermo/ejemplo.py\nfrom pathlib import Path\n\ndef load_items(directory):\n    items = []\n    for entry in directory:\n        if entry.is_file():\n            items.append(entry)\n    return sorted(items)\n\ndef main():\n    print(\"Hola desde neo_ws 1.0!\")\n    for item in load_items(\"/bin\"):\n        print(f\" - {item}\")\n\nif __name__ == \"__main__\":\n    main()\n"
                .to_vec(),
        ),
    );
    nodos.insert(
        "/home/guillermo/notas.txt".to_string(),
        Nodo::Archivo(
            b"Notas de neo_ws 2.0\n- Kernel monolitico/framekernel en Rust (0-lag 250 Hz PIT + cursor 16x16)\n- Disco persistente real NeoFS (/dev/sdb -> neo_disk.img)\n- Red real Intel e1000 + ARP/IPv4/ICMP/DNS/TCP + Navegador NeoWeb\n- Audio real PCI Intel AC97 (/dev/dsp) + Motor FPS NeoDoom 3D\n- 3 GB RAM + 4 vCPUs WHPX + 41 syscalls Linux (musl/std OK)\n"
                .to_vec(),
        ),
    );
    nodos.insert(
        "/etc/neo_ws/theme.conf".to_string(),
        Nodo::Archivo(
            b"# Configuracion de estilo de neo_ws 2.0 (persistente en NeoFS)\nwallpaper=0\nopacity=78\nblur=1\ndark=1\nbrightness=100\naccent=#3B82F6\n"
                .to_vec(),
        ),
    );
    nodos.insert(
        "/etc/os-release".to_string(),
        Nodo::Archivo(
            b"NAME=\"neo_ws\"\nVERSION=\"2.0\"\nID=neo_ws\nPRETTY_NAME=\"neo_ws 2.0 (NeoFS + Intel e1000 + AC97 + Smooth Glass Desktop)\"\n"
                .to_vec(),
        ),
    );
    nodos.insert(
        "/etc/motd".to_string(),
        Nodo::Archivo(
            b"=== neo_ws 2.0 (Modo Consola | escribe 'desktop' para volver a GUI) ===\nComandos: help, ls, cat, hola, hola_std, forktest, desktop\n"
                .to_vec(),
        ),
    );
    nodos.insert("/dev/null".to_string(), Nodo::DevNull);
    nodos.insert("/dev/zero".to_string(), Nodo::DevZero);
    nodos.insert("/dev/fb0".to_string(), Nodo::DevFb0);
    nodos.insert("/dev/input/events".to_string(), Nodo::DevInputEvents);
    nodos.insert("/dev/sda".to_string(), Nodo::DevSda);
    nodos.insert("/dev/net".to_string(), Nodo::DevNet);
    nodos.insert("/dev/dsp".to_string(), Nodo::DevDsp);
    nodos.insert("/proc/uptime".to_string(), Nodo::ProcUptime);
    nodos.insert("/proc/ps".to_string(), Nodo::ProcPs);
    nodos.insert("/proc/disk".to_string(), Nodo::ProcDisk);
    nodos.insert("/proc/meminfo".to_string(), Nodo::ProcMeminfo);
    nodos.insert("/proc/time".to_string(), Nodo::ProcTime);
    nodos.insert("/proc/net".to_string(), Nodo::ProcNet);

    // Sincronizar con disco persistente NeoFS (/dev/sdb -> neo_disk.img)
    if let Some(recien_formateado) = neofs::init() {
        if recien_formateado {
            // Sembrar archivos iniciales en el disco nuevo para que existan tras reiniciar
            for (ruta, nodo) in nodos.iter() {
                if neofs::es_ruta_persistente(ruta) {
                    if let Nodo::Archivo(datos) = nodo {
                        neofs::guardar_archivo(ruta, datos);
                    }
                }
            }
        } else {
            // Cargar todos los archivos guardados del disco real (sobrescriben los por defecto)
            for (ruta, datos) in neofs::cargar_todos_los_archivos() {
                nodos.insert(ruta, Nodo::Archivo(datos));
            }
        }
    }

    *NODOS.lock() = Some(nodos);
}

pub fn resolver_ruta(cwd: &str, ruta: &str) -> String {
    let combinada = if ruta.starts_with('/') {
        ruta.to_string()
    } else if cwd == "/" {
        format!("/{}", ruta)
    } else {
        format!("{}/{}", cwd, ruta)
    };

    let mut partes: Vec<&str> = Vec::new();
    for p in combinada.split('/') {
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

pub fn es_directorio(ruta: &str) -> Result<bool, i64> {
    let norm = resolver_ruta("/", ruta);
    let guard = NODOS.lock();
    let Some(nodos) = guard.as_ref() else {
        return Err(ENOENT);
    };
    match nodos.get(&norm) {
        Some(Nodo::Directorio) => Ok(true),
        Some(_) => Ok(false),
        None => Err(ENOENT),
    }
}

/// Lee un archivo completo desde el kernel (por ejemplo, para cargar `/bin/sh` o en `execve`).
pub fn leer_archivo_kernel(ruta: &str) -> Option<Vec<u8>> {
    let norm = resolver_ruta("/", ruta);
    let guard = NODOS.lock();
    let nodos = guard.as_ref()?;
    match nodos.get(&norm) {
        Some(Nodo::Archivo(datos)) => Some(datos.clone()),
        Some(Nodo::ArchivoEstatico(datos)) => Some(datos.to_vec()),
        _ => None,
    }
}

pub fn preparar_apertura(cwd: &str, ruta: &str, flags: u64) -> Result<Descriptor, i64> {
    let norm = resolver_ruta(cwd, ruta);
    let modo = flags & 3;
    let puede_leer = modo == 0 || modo == O_RDWR;
    let puede_escribir = modo == O_WRONLY || modo == O_RDWR;

    let mut guard = NODOS.lock();
    let Some(nodos) = guard.as_mut() else {
        return Err(ENOENT);
    };

    if !nodos.contains_key(&norm) {
        if (flags & O_CREAT) == 0 {
            return Err(ENOENT);
        }
        let padre = directorio_padre(&norm);
        match nodos.get(&padre) {
            Some(Nodo::Directorio) => {
                nodos.insert(norm.clone(), Nodo::Archivo(Vec::new()));
            }
            _ => return Err(ENOENT),
        }
    }

    let (es_directorio, posicion) = match nodos.get_mut(&norm) {
        Some(Nodo::Directorio) => {
            if puede_escribir {
                return Err(EISDIR);
            }
            (true, 0)
        }
        Some(Nodo::Archivo(datos)) => {
            if puede_escribir && (flags & O_TRUNC) != 0 {
                datos.clear();
                if neofs::es_ruta_persistente(&norm) {
                    neofs::guardar_archivo(&norm, datos);
                }
            }
            let pos = if (flags & O_APPEND) != 0 {
                datos.len()
            } else {
                0
            };
            (false, pos)
        }
        Some(Nodo::ArchivoEstatico(_)) => {
            if puede_escribir {
                return Err(EACCES);
            }
            (false, 0)
        }
        Some(Nodo::DevFb0) => return Ok(Descriptor::DevFb0),
        Some(Nodo::DevInputEvents) => return Ok(Descriptor::DevInputEvents),
        Some(
            Nodo::DevNull
            | Nodo::DevZero
            | Nodo::DevSda
            | Nodo::DevNet
            | Nodo::DevDsp
            | Nodo::ProcUptime
            | Nodo::ProcPs
            | Nodo::ProcDisk
            | Nodo::ProcMeminfo
            | Nodo::ProcTime
            | Nodo::ProcNet,
        ) => (false, 0),
        None => return Err(ENOENT),
    };

    Ok(Descriptor::Archivo(Arc::new(Mutex::new(EstadoArchivo {
        ruta: norm,
        posicion,
        puede_leer,
        puede_escribir,
        es_directorio,
    }))))
}

pub fn leer_archivo(est_arc: &Arc<Mutex<EstadoArchivo>>, dst: &mut [u8]) -> i64 {
    let (ruta, pos, puede_leer, es_dir) = {
        let e = est_arc.lock();
        (e.ruta.clone(), e.posicion, e.puede_leer, e.es_directorio)
    };
    if !puede_leer {
        return EACCES;
    }
    if es_dir {
        return EISDIR;
    }

    // `/proc/ps` se resuelve sin mantener bloqueado `NODOS`.
    let es_ps = {
        let guard = NODOS.lock();
        matches!(
            guard.as_ref().and_then(|n| n.get(&ruta)),
            Some(Nodo::ProcPs)
        )
    };
    let leidos = if es_ps {
        let texto = proceso::formatear_ps();
        let b = texto.as_bytes();
        if pos >= b.len() {
            0
        } else {
            let cant = dst.len().min(b.len() - pos);
            dst[..cant].copy_from_slice(&b[pos..pos + cant]);
            cant
        }
    } else {
        let guard = NODOS.lock();
        let Some(nodos) = guard.as_ref() else {
            return EBADF;
        };
        match nodos.get(&ruta) {
            Some(Nodo::Archivo(datos)) => {
                if pos >= datos.len() {
                    0
                } else {
                    let cant = dst.len().min(datos.len() - pos);
                    dst[..cant].copy_from_slice(&datos[pos..pos + cant]);
                    cant
                }
            }
            Some(Nodo::ArchivoEstatico(datos)) => {
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
            Some(Nodo::DevSda) => disco::leer_bytes(pos, dst),
            Some(Nodo::DevNet) => net::leer_respuesta_dev_net(pos, dst),
            Some(Nodo::DevDsp) => {
                let s: &[u8] = if audio::disponible() { b"AC97 48kHz OK\n" } else { b"Sin audio\n" };
                if pos >= s.len() { 0 } else {
                    let c = dst.len().min(s.len() - pos);
                    dst[..c].copy_from_slice(&s[pos..pos + c]);
                    c
                }
            }
            Some(Nodo::ProcNet) => {
                let texto = net::formatear_proc_net();
                let b = texto.as_bytes();
                if pos >= b.len() {
                    0
                } else {
                    let cant = dst.len().min(b.len() - pos);
                    dst[..cant].copy_from_slice(&b[pos..pos + cant]);
                    cant
                }
            }
            Some(Nodo::ProcDisk) => {
                let texto = disco::formatear_proc_disk();
                let b = texto.as_bytes();
                if pos >= b.len() {
                    0
                } else {
                    let cant = dst.len().min(b.len() - pos);
                    dst[..cant].copy_from_slice(&b[pos..pos + cant]);
                    cant
                }
            }
            Some(Nodo::ProcMeminfo) => {
                let texto = format!("{} MiB\n", memoria::ram_total_mb());
                let b = texto.as_bytes();
                if pos >= b.len() {
                    0
                } else {
                    let cant = dst.len().min(b.len() - pos);
                    dst[..cant].copy_from_slice(&b[pos..pos + cant]);
                    cant
                }
            }
            Some(Nodo::ProcTime) => {
                let texto = reloj::formatear_proc_time();
                let b = texto.as_bytes();
                if pos >= b.len() {
                    0
                } else {
                    let cant = dst.len().min(b.len() - pos);
                    dst[..cant].copy_from_slice(&b[pos..pos + cant]);
                    cant
                }
            }
            Some(Nodo::ProcUptime) => {
                let t = interrupciones::ticks();
                let ms = interrupciones::ms_transcurridos();
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
        }
    };

    est_arc.lock().posicion += leidos;
    leidos as i64
}

pub fn escribir_archivo(est_arc: &Arc<Mutex<EstadoArchivo>>, src: &[u8]) -> i64 {
    let (ruta, pos, puede_escribir, es_dir) = {
        let e = est_arc.lock();
        (
            e.ruta.clone(),
            e.posicion,
            e.puede_escribir,
            e.es_directorio,
        )
    };
    if !puede_escribir {
        return EACCES;
    }
    if es_dir {
        return EISDIR;
    }

    let mut guard = NODOS.lock();
    let Some(nodos) = guard.as_mut() else {
        return EBADF;
    };

    let mut copia_disco: Option<Vec<u8>> = None;
    match nodos.get_mut(&ruta) {
        Some(Nodo::Archivo(datos)) => {
            if pos + src.len() > datos.len() {
                datos.resize(pos + src.len(), 0);
            }
            datos[pos..pos + src.len()].copy_from_slice(src);
            if neofs::es_ruta_persistente(&ruta) {
                copia_disco = Some(datos.clone());
            }
        }
        Some(Nodo::DevNull | Nodo::DevZero) => {}
        Some(Nodo::DevNet) => {
            drop(guard);
            net::procesar_comando_dev_net(src);
            est_arc.lock().posicion = 0;
            return src.len() as i64;
        }
        Some(Nodo::DevDsp) => {
            drop(guard);
            audio::procesar_comando_dsp(src);
            return src.len() as i64;
        }
        _ => return EBADF,
    }
    drop(guard);

    if let Some(datos_disco) = copia_disco {
        neofs::guardar_archivo(&ruta, &datos_disco);
    }

    est_arc.lock().posicion += src.len();
    src.len() as i64
}

/// Lee desde un pipe: devuelve `Ok(n)` (0 = EOF si no quedan escritores) o `Err(())` si debe ceder CPU.
pub fn leer_pipe(pipe: &PipeLectura, dst: &mut [u8]) -> Result<i64, ()> {
    let mut p = pipe.0.lock();
    if p.datos.is_empty() {
        if p.escritores == 0 {
            return Ok(0); // EOF real: todos los escritores cerraron su extremo.
        }
        return Err(()); // Hay escritores vivos: ceder CPU y reintentar.
    }
    let mut n = 0usize;
    while n < dst.len() {
        let Some(b) = p.datos.pop_front() else {
            break;
        };
        dst[n] = b;
        n += 1;
    }
    Ok(n as i64)
}

pub fn escribir_pipe(pipe: &PipeEscritura, src: &[u8]) -> i64 {
    let mut p = pipe.0.lock();
    if p.lectores == 0 {
        return EPIPE;
    }
    for &b in src {
        p.datos.push_back(b);
    }
    src.len() as i64
}

pub fn lseek_descriptor(desc: &Descriptor, offset: i64, whence: u64) -> i64 {
    let Descriptor::Archivo(est_arc) = desc else {
        return ESPIPE;
    };
    let mut e = est_arc.lock();
    let tamano = {
        let guard = NODOS.lock();
        match guard.as_ref().and_then(|n| n.get(&e.ruta)) {
            Some(Nodo::Archivo(d)) => d.len() as i64,
            _ => 0,
        }
    };
    let nueva = match whence {
        0 => offset,                  // SEEK_SET
        1 => e.posicion as i64 + offset, // SEEK_CUR
        2 => tamano + offset,         // SEEK_END
        _ => return EINVAL,
    };
    if nueva < 0 {
        return EINVAL;
    }
    e.posicion = nueva as usize;
    nueva
}

pub fn stat_ruta(cwd: &str, ruta: &str) -> Result<StatLinux, i64> {
    let norm = resolver_ruta(cwd, ruta);
    let guard = NODOS.lock();
    let Some(nodos) = guard.as_ref() else {
        return Err(ENOENT);
    };
    let Some(nodo) = nodos.get(&norm) else {
        return Err(ENOENT);
    };
    let mut st = StatLinux::default();
    st.st_ino = 2;
    st.st_nlink = 1;
    st.st_blksize = 4096;
    match nodo {
        Nodo::Directorio => {
            st.st_mode = S_IFDIR | 0o755;
            st.st_size = 4096;
        }
        Nodo::Archivo(datos) => {
            st.st_mode = S_IFREG | 0o644;
            st.st_size = datos.len() as i64;
            st.st_blocks = ((datos.len() as i64) + 511) / 512;
        }
        Nodo::ArchivoEstatico(datos) => {
            st.st_mode = S_IFREG | 0o755;
            st.st_size = datos.len() as i64;
            st.st_blocks = ((datos.len() as i64) + 511) / 512;
        }
        Nodo::DevNull
        | Nodo::DevZero
        | Nodo::DevFb0
        | Nodo::DevInputEvents
        | Nodo::DevNet
        | Nodo::DevDsp => {
            st.st_mode = S_IFCHR | 0o666;
            if matches!(nodo, Nodo::DevFb0) {
                st.st_size = fb::TAMANO_FB as i64;
            }
        }
        Nodo::DevSda => {
            st.st_mode = S_IFREG | 0o444;
            st.st_size = disco::tamano_bytes() as i64;
        }
        Nodo::ProcUptime
        | Nodo::ProcPs
        | Nodo::ProcDisk
        | Nodo::ProcMeminfo
        | Nodo::ProcTime
        | Nodo::ProcNet => {
            st.st_mode = S_IFREG | 0o444;
        }
    }
    Ok(st)
}

pub fn stat_descriptor(desc: &Descriptor) -> Result<StatLinux, i64> {
    match desc {
        Descriptor::TtyEntrada | Descriptor::TtySalida | Descriptor::DevInputEvents => {
            let mut st = StatLinux::default();
            st.st_ino = 1;
            st.st_nlink = 1;
            st.st_mode = S_IFCHR | 0o666;
            st.st_blksize = 4096;
            Ok(st)
        }
        Descriptor::DevFb0 => {
            let mut st = StatLinux::default();
            st.st_ino = 1;
            st.st_nlink = 1;
            st.st_mode = S_IFCHR | 0o666;
            st.st_size = fb::TAMANO_FB as i64;
            st.st_blksize = 4096;
            Ok(st)
        }
        Descriptor::PipeLectura(_) | Descriptor::PipeEscritura(_) => {
            let mut st = StatLinux::default();
            st.st_ino = 1;
            st.st_nlink = 1;
            st.st_mode = S_IFIFO | 0o600;
            st.st_blksize = 4096;
            Ok(st)
        }
        Descriptor::Archivo(est_arc) => {
            let ruta = est_arc.lock().ruta.clone();
            stat_ruta("/", &ruta)
        }
    }
}

pub fn getdents64_archivo(est_arc: &Arc<Mutex<EstadoArchivo>>, dst: &mut [u8]) -> i64 {
    let (dir, inicio_idx, es_dir) = {
        let e = est_arc.lock();
        (e.ruta.clone(), e.posicion, e.es_directorio)
    };
    if !es_dir {
        return ENOTDIR;
    }

    let guard = NODOS.lock();
    let Some(nodos) = guard.as_ref() else {
        return EBADF;
    };

    let mut hijos: Vec<(String, u8)> = Vec::new();
    for (ruta, nodo) in nodos {
        if ruta == &dir {
            continue;
        }
        if directorio_padre(ruta) == dir {
            let nombre = ruta.rsplit('/').next().unwrap_or("");
            let dtype = match nodo {
                Nodo::Directorio => DT_DIR,
                Nodo::Archivo(_)
                | Nodo::ArchivoEstatico(_)
                | Nodo::ProcUptime
                | Nodo::ProcPs
                | Nodo::ProcDisk
                | Nodo::ProcMeminfo
                | Nodo::ProcTime
                | Nodo::ProcNet => DT_REG,
                Nodo::DevNull
                | Nodo::DevZero
                | Nodo::DevFb0
                | Nodo::DevInputEvents
                | Nodo::DevSda
                | Nodo::DevNet
                | Nodo::DevDsp => DT_CHR,
            };
            hijos.push((nombre.to_string(), dtype));
        }
    }

    let mut escrito = 0usize;
    let mut nuevo_idx = inicio_idx;

    for (idx, (nombre, dtype)) in hijos.iter().enumerate().skip(inicio_idx) {
        let nb = nombre.as_bytes();
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
    drop(guard);

    est_arc.lock().posicion = nuevo_idx;
    escrito as i64
}
