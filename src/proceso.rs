// Gestor de procesos: CR3 aislado, FS.base (TLS), cwd, mmap, pipe/dup2, fork, execve, wait4.
use crate::fs::{self, Descriptor, TablaFds};
use crate::memoria::{self, EspacioDirecciones};
use crate::{elf, fb, println};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;
use x86_64::registers::model_specific::{Efer, EferFlags, FsBase};
use x86_64::structures::paging::{Page, PageTableFlags, PhysFrame, Size4KiB};
use x86_64::{PhysAddr, VirtAddr};

const ENOENT: i64 = -2;
const ENOEXEC: i64 = -8;
const EBADF: i64 = -9;
const ECHILD: i64 = -10;
const ENOMEM: i64 = -12;
const ENOTDIR: i64 = -20;
const EINVAL: i64 = -22;

const LIMITE_USUARIO: u64 = 0x0000_8000_0000_0000;
const MAX_BRK_CRECIMIENTO: u64 = 16 * 1024 * 1024;
const BASE_MMAP_INICIAL: u64 = 0x0000_2000_0000;

const PROT_WRITE: u64 = 2;
const PROT_EXEC: u64 = 4;
const MAP_FIXED: u64 = 0x10;
const MAP_ANONYMOUS: u64 = 0x20;

const ARCH_SET_FS: u64 = 0x1002;
const ARCH_GET_FS: u64 = 0x1003;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ContextoSyscall {
    pub rax: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub r10: u64,
    pub r8: u64,
    pub r9: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub user_rip: u64,
    pub user_rflags: u64,
    pub user_rsp: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EstadoProceso {
    Ejecutando,
    Listo,
    EsperandoHijo { pid_esperado: i64, wstatus_ptr: u64 },
    Terminado(i64),
}

pub struct Proceso {
    pub pid: u64,
    pub ppid: u64,
    pub nombre: String,
    pub cwd: String,
    pub estado: EstadoProceso,
    pub espacio: EspacioDirecciones,
    pub fds: TablaFds,
    pub brk_inicio: u64,
    pub brk_actual: u64,
    pub mmap_siguiente: u64,
    pub fs_base: u64,
    pub contexto: ContextoSyscall,
}

impl Proceso {
    fn activar_hardware(&self) {
        self.espacio.activar();
        FsBase::write(VirtAddr::new(self.fs_base));
    }
}

struct TablaProcesos {
    lista: Vec<Proceso>,
    actual: u64,
    siguiente_pid: u64,
}

static PROCESOS: Mutex<TablaProcesos> = Mutex::new(TablaProcesos {
    lista: Vec::new(),
    actual: 0,
    siguiente_pid: 1,
});

/// Crea el proceso 1 (init, normalmente `/bin/sh`), carga su ELF en su propio CR3 y lo activa.
pub fn crear_init(ruta: &str) -> Result<(u64, u64, usize, u64), &'static str> {
    let binario = fs::leer_archivo_kernel(ruta).ok_or("No se encontro el binario inicial")?;
    let mut espacio = EspacioDirecciones::nuevo_vacio()?;
    let prog = elf::cargar(&binario, &mut espacio, &[ruta])?;

    espacio.activar();
    FsBase::write(VirtAddr::new(0));

    let mut tabla = PROCESOS.lock();
    if tabla.lista.is_empty() {
        tabla.siguiente_pid = 1;
    }
    let pid = tabla.siguiente_pid;
    tabla.siguiente_pid += 1;
    tabla.actual = pid;

    let mut ctx = ContextoSyscall::default();
    ctx.user_rip = prog.punto_entrada;
    ctx.user_rsp = prog.pila_usuario;
    ctx.user_rflags = 0x202;

    tabla.lista.push(Proceso {
        pid,
        ppid: 0,
        nombre: ruta.to_string(),
        cwd: "/".to_string(),
        estado: EstadoProceso::Ejecutando,
        espacio,
        fds: TablaFds::nueva(),
        brk_inicio: prog.fin_datos,
        brk_actual: prog.fin_datos,
        mmap_siguiente: BASE_MMAP_INICIAL,
        fs_base: 0,
        contexto: ctx,
    });

    Ok((
        prog.punto_entrada,
        prog.pila_usuario,
        prog.segmentos_cargados,
        prog.fin_datos,
    ))
}

pub fn pid_actual() -> u64 {
    PROCESOS.lock().actual
}

pub fn ppid_actual() -> u64 {
    let tabla = PROCESOS.lock();
    tabla
        .lista
        .iter()
        .find(|p| p.pid == tabla.actual)
        .map(|p| p.ppid)
        .unwrap_or(0)
}

pub fn cwd_actual() -> String {
    let tabla = PROCESOS.lock();
    tabla
        .lista
        .iter()
        .find(|p| p.pid == tabla.actual)
        .map(|p| p.cwd.clone())
        .unwrap_or_else(|| "/".to_string())
}

pub fn hacer_chdir(ruta: &str) -> i64 {
    let cwd = cwd_actual();
    let resuelta = fs::resolver_ruta(&cwd, ruta);
    match fs::es_directorio(&resuelta) {
        Ok(true) => {
            let mut tabla = PROCESOS.lock();
            let pid = tabla.actual;
            if let Some(p) = tabla.lista.iter_mut().find(|p| p.pid == pid) {
                p.cwd = resuelta;
                0
            } else {
                ENOENT
            }
        }
        Ok(false) => ENOTDIR,
        Err(e) => e,
    }
}

pub fn abrir_fd(ruta: &str, flags: u64) -> i64 {
    let cwd = cwd_actual();
    let desc = match fs::preparar_apertura(&cwd, ruta, flags) {
        Ok(d) => d,
        Err(e) => return e,
    };
    let mut tabla = PROCESOS.lock();
    let pid = tabla.actual;
    if let Some(p) = tabla.lista.iter_mut().find(|p| p.pid == pid) {
        p.fds.insertar(desc)
    } else {
        EBADF
    }
}

pub fn cerrar_fd(fd: usize) -> i64 {
    let mut tabla = PROCESOS.lock();
    let pid = tabla.actual;
    if let Some(p) = tabla.lista.iter_mut().find(|p| p.pid == pid) {
        p.fds.cerrar(fd)
    } else {
        EBADF
    }
}

pub fn hacer_dup(viejo: usize) -> i64 {
    let mut tabla = PROCESOS.lock();
    let pid = tabla.actual;
    if let Some(p) = tabla.lista.iter_mut().find(|p| p.pid == pid) {
        p.fds.dup(viejo)
    } else {
        EBADF
    }
}

pub fn hacer_dup_desde(viejo: usize, minimo: usize) -> i64 {
    let mut tabla = PROCESOS.lock();
    let pid = tabla.actual;
    if let Some(p) = tabla.lista.iter_mut().find(|p| p.pid == pid) {
        let Some(desc) = p.fds.fds.get(viejo).and_then(|d| d.clone()) else {
            return EBADF;
        };
        p.fds.insertar_desde(minimo, desc)
    } else {
        EBADF
    }
}

pub fn hacer_dup2(viejo: usize, nuevo: usize) -> i64 {
    let mut tabla = PROCESOS.lock();
    let pid = tabla.actual;
    if let Some(p) = tabla.lista.iter_mut().find(|p| p.pid == pid) {
        p.fds.dup2(viejo, nuevo)
    } else {
        EBADF
    }
}

pub fn hacer_pipe() -> Result<(usize, usize), i64> {
    let (lectura, escritura) = fs::crear_pipe();
    let mut tabla = PROCESOS.lock();
    let pid = tabla.actual;
    let Some(p) = tabla.lista.iter_mut().find(|p| p.pid == pid) else {
        return Err(EBADF);
    };
    let fd_r = p.fds.insertar(lectura);
    if fd_r < 0 {
        return Err(fd_r);
    }
    let fd_w = p.fds.insertar(escritura);
    if fd_w < 0 {
        p.fds.cerrar(fd_r as usize);
        return Err(fd_w);
    }
    Ok((fd_r as usize, fd_w as usize))
}

pub fn obtener_descriptor(fd: usize) -> Option<Descriptor> {
    let tabla = PROCESOS.lock();
    let p = tabla.lista.iter().find(|p| p.pid == tabla.actual)?;
    p.fds.fds.get(fd).and_then(|d| d.clone())
}

pub fn hacer_arch_prctl(codigo: u64, addr: u64) -> i64 {
    let mut tabla = PROCESOS.lock();
    let pid = tabla.actual;
    let Some(p) = tabla.lista.iter_mut().find(|p| p.pid == pid) else {
        return EINVAL;
    };
    match codigo {
        ARCH_SET_FS => {
            if addr >= LIMITE_USUARIO {
                return EINVAL;
            }
            p.fs_base = addr;
            FsBase::write(VirtAddr::new(addr));
            0
        }
        ARCH_GET_FS => {
            if addr >= 0x1000 && addr + 8 < LIMITE_USUARIO {
                unsafe { (addr as *mut u64).write(p.fs_base) };
                0
            } else {
                EINVAL
            }
        }
        _ => EINVAL,
    }
}

/// Implementacion de `sys_mmap` anonimo o sobre `/dev/fb0` (MMIO) en el CR3 del proceso actual.
pub fn hacer_mmap(addr: u64, len: u64, prot: u64, flags: u64, fd: i64) -> i64 {
    if len == 0 || len > 32 * 1024 * 1024 {
        return EINVAL;
    }

    // Caso 1: mmap sobre /dev/fb0 -> activa modo grafico 800x600x32bpp y mapea el BAR0 fisico.
    if (flags & MAP_ANONYMOUS) == 0 && fd >= 0 {
        let es_fb0 = matches!(obtener_descriptor(fd as usize), Some(Descriptor::DevFb0));
        if !es_fb0 {
            return EINVAL;
        }
        let Some(info) = fb::activar_modo_grafico() else {
            return EINVAL;
        };
        let tam_real = (len as usize).min(info.tamano);
        let paginas = ((tam_real + 4095) / 4096) as u64;
        let tam_alineado = paginas * 4096;

        let mut tabla = PROCESOS.lock();
        let pid = tabla.actual;
        let Some(p) = tabla.lista.iter_mut().find(|p| p.pid == pid) else {
            return ENOMEM;
        };
        let base = p.mmap_siguiente;
        p.mmap_siguiente = base + tam_alineado;
        if base.saturating_add(tam_alineado) >= LIMITE_USUARIO {
            return ENOMEM;
        }

        let mut pflags =
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
        if Efer::read().contains(EferFlags::NO_EXECUTE_ENABLE) {
            pflags |= PageTableFlags::NO_EXECUTE;
        }

        for i in 0..paginas {
            let pagina = Page::<Size4KiB>::containing_address(VirtAddr::new(base + i * 4096));
            let frame_fis =
                PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(info.phys_addr + i * 4096));
            if p.espacio.mapear_mmio(pagina, frame_fis, pflags).is_err() {
                return ENOMEM;
            }
        }
        x86_64::instructions::tlb::flush_all();
        return base as i64;
    }

    if (flags & MAP_ANONYMOUS) == 0 && fd != -1 {
        return EINVAL;
    }

    let mut tabla = PROCESOS.lock();
    let pid = tabla.actual;
    let Some(p) = tabla.lista.iter_mut().find(|p| p.pid == pid) else {
        return ENOMEM;
    };

    let paginas = (len + 4095) / 4096;
    let tam_alineado = paginas * 4096;

    let base = if (flags & MAP_FIXED) != 0 && addr >= 0x10000 {
        addr & !4095
    } else {
        let b = p.mmap_siguiente;
        p.mmap_siguiente = b + tam_alineado;
        b
    };

    if base.saturating_add(tam_alineado) >= LIMITE_USUARIO {
        return ENOMEM;
    }

    let mut pflags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
    if (prot & PROT_WRITE) != 0 {
        pflags |= PageTableFlags::WRITABLE;
    }
    if (prot & PROT_EXEC) == 0 && Efer::read().contains(EferFlags::NO_EXECUTE_ENABLE) {
        pflags |= PageTableFlags::NO_EXECUTE;
    }

    for i in 0..paginas {
        let pagina = Page::<Size4KiB>::containing_address(VirtAddr::new(base + i * 4096));
        if p.espacio.mapear_pagina(pagina, pflags).is_err() {
            return ENOMEM;
        }
    }
    x86_64::instructions::tlb::flush_all();
    base as i64
}

/// Implementacion de `sys_brk` en el espacio de direcciones privado del proceso actual.
pub fn hacer_brk(nueva_dir: u64) -> u64 {
    let mut tabla = PROCESOS.lock();
    let pid = tabla.actual;
    let Some(p) = tabla.lista.iter_mut().find(|p| p.pid == pid) else {
        return 0;
    };

    if nueva_dir == 0 || nueva_dir < p.brk_inicio {
        return p.brk_actual;
    }
    if nueva_dir > p.brk_inicio + MAX_BRK_CRECIMIENTO || nueva_dir >= LIMITE_USUARIO {
        return p.brk_actual;
    }
    if nueva_dir > p.brk_actual {
        let pag_inicio = (p.brk_actual + 4095) & !4095;
        let pag_fin = (nueva_dir + 4095) & !4095;
        let mut flags =
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
        if Efer::read().contains(EferFlags::NO_EXECUTE_ENABLE) {
            flags |= PageTableFlags::NO_EXECUTE;
        }
        let mut dir = pag_inicio;
        while dir < pag_fin {
            let pagina = Page::<Size4KiB>::containing_address(VirtAddr::new(dir));
            if p.espacio.mapear_pagina(pagina, flags).is_err() {
                return p.brk_actual;
            }
            dir += 4096;
        }
        x86_64::instructions::tlb::flush_all();
    }
    p.brk_actual = nueva_dir;
    p.brk_actual
}

/// Implementacion de `fork()` (Linux syscall 57): clona CR3, FDs, cwd, TLS y contexto.
pub fn hacer_fork(ctx: &ContextoSyscall) -> i64 {
    let mut tabla = PROCESOS.lock();
    let pid_padre = tabla.actual;
    let Some(idx_padre) = tabla.lista.iter().position(|p| p.pid == pid_padre) else {
        return ENOMEM;
    };

    let espacio_hijo = match tabla.lista[idx_padre].espacio.clonar() {
        Ok(e) => e,
        Err(_) => return ENOMEM,
    };

    let fds_hijo = tabla.lista[idx_padre].fds.clone();
    let nombre_hijo = tabla.lista[idx_padre].nombre.clone();
    let cwd_hijo = tabla.lista[idx_padre].cwd.clone();
    let brk_inicio = tabla.lista[idx_padre].brk_inicio;
    let brk_actual = tabla.lista[idx_padre].brk_actual;
    let mmap_siguiente = tabla.lista[idx_padre].mmap_siguiente;
    let fs_base = tabla.lista[idx_padre].fs_base;

    let mut ctx_hijo = *ctx;
    ctx_hijo.rax = 0;

    let pid_hijo = tabla.siguiente_pid;
    tabla.siguiente_pid += 1;

    tabla.lista.push(Proceso {
        pid: pid_hijo,
        ppid: pid_padre,
        nombre: nombre_hijo,
        cwd: cwd_hijo,
        estado: EstadoProceso::Listo,
        espacio: espacio_hijo,
        fds: fds_hijo,
        brk_inicio,
        brk_actual,
        mmap_siguiente,
        fs_base,
        contexto: ctx_hijo,
    });

    pid_hijo as i64
}

/// Implementacion de `execve()` (Linux syscall 59): reemplaza el proceso actual por otro ELF.
pub fn hacer_execve(ruta: &str, argv: &[String], ctx: &mut ContextoSyscall) -> i64 {
    let cwd = cwd_actual();
    let ruta_abs = fs::resolver_ruta(&cwd, ruta);
    let Some(binario) = fs::leer_archivo_kernel(&ruta_abs) else {
        return ENOENT;
    };

    let mut espacio_nuevo = match EspacioDirecciones::nuevo_vacio() {
        Ok(e) => e,
        Err(_) => return ENOMEM,
    };

    let refs: Vec<&str> = if argv.is_empty() {
        alloc::vec![ruta_abs.as_str()]
    } else {
        argv.iter().map(|s| s.as_str()).collect()
    };

    let prog = match elf::cargar(&binario, &mut espacio_nuevo, &refs) {
        Ok(p) => p,
        Err(_) => {
            espacio_nuevo.liberar();
            return ENOEXEC;
        }
    };

    let mut tabla = PROCESOS.lock();
    let pid = tabla.actual;
    let Some(p) = tabla.lista.iter_mut().find(|p| p.pid == pid) else {
        espacio_nuevo.liberar();
        return ENOEXEC;
    };

    espacio_nuevo.activar();
    FsBase::write(VirtAddr::new(0));
    let espacio_viejo = core::mem::replace(&mut p.espacio, espacio_nuevo);
    espacio_viejo.liberar();

    p.nombre = ruta_abs;
    p.brk_inicio = prog.fin_datos;
    p.brk_actual = prog.fin_datos;
    p.mmap_siguiente = BASE_MMAP_INICIAL;
    p.fs_base = 0;

    *ctx = ContextoSyscall::default();
    ctx.user_rip = prog.punto_entrada;
    ctx.user_rsp = prog.pila_usuario;
    ctx.user_rflags = 0x202;
    0
}

fn coincide_espera(pid_esperado: i64, pid_hijo: u64) -> bool {
    pid_esperado == -1 || pid_esperado == 0 || pid_esperado == pid_hijo as i64
}

fn escribir_wstatus(wstatus_ptr: u64, codigo: i64) {
    if wstatus_ptr >= 0x1000 && wstatus_ptr + 4 < LIMITE_USUARIO {
        let valor: i32 = ((codigo & 0xff) << 8) as i32;
        unsafe {
            (wstatus_ptr as *mut i32).write(valor);
        }
    }
}

/// Implementacion de `wait4()` (Linux syscall 61): espera a que termine un proceso hijo.
pub fn hacer_wait4(pid_esperado: i64, wstatus_ptr: u64, ctx: &mut ContextoSyscall) {
    let mut tabla = PROCESOS.lock();
    let pid_padre = tabla.actual;

    let tiene_hijo = tabla
        .lista
        .iter()
        .any(|p| p.ppid == pid_padre && coincide_espera(pid_esperado, p.pid));
    if !tiene_hijo {
        ctx.rax = ECHILD as u64;
        return;
    }

    if let Some(idx_z) = tabla.lista.iter().position(|p| {
        p.ppid == pid_padre
            && coincide_espera(pid_esperado, p.pid)
            && matches!(p.estado, EstadoProceso::Terminado(_))
    }) {
        let hijo = tabla.lista.remove(idx_z);
        let codigo = match hijo.estado {
            EstadoProceso::Terminado(c) => c,
            _ => 0,
        };
        hijo.espacio.liberar();
        escribir_wstatus(wstatus_ptr, codigo);
        ctx.rax = hijo.pid;
        return;
    }

    if let Some(padre) = tabla.lista.iter_mut().find(|p| p.pid == pid_padre) {
        padre.contexto = *ctx;
        padre.estado = EstadoProceso::EsperandoHijo {
            pid_esperado,
            wstatus_ptr,
        };
    }

    let idx_sig = tabla
        .lista
        .iter()
        .position(|p| {
            p.ppid == pid_padre
                && coincide_espera(pid_esperado, p.pid)
                && p.estado == EstadoProceso::Listo
        })
        .or_else(|| {
            tabla
                .lista
                .iter()
                .position(|p| p.estado == EstadoProceso::Listo)
        });

    let Some(idx_sig) = idx_sig else {
        panic!("wait4: no hay procesos listos para ejecutar");
    };

    tabla.lista[idx_sig].estado = EstadoProceso::Ejecutando;
    tabla.actual = tabla.lista[idx_sig].pid;
    tabla.lista[idx_sig].activar_hardware();
    *ctx = tabla.lista[idx_sig].contexto;
}

/// Cede el CPU a otro proceso listo y retrocede RIP 2 bytes para reintentar la syscall al volver.
pub fn ceder_y_reintentar(ctx: &mut ContextoSyscall) -> bool {
    let mut tabla = PROCESOS.lock();
    let pid_actual = tabla.actual;
    let Some(idx_sig) = tabla
        .lista
        .iter()
        .position(|p| p.pid != pid_actual && p.estado == EstadoProceso::Listo)
    else {
        return false;
    };

    if let Some(p) = tabla.lista.iter_mut().find(|p| p.pid == pid_actual) {
        p.contexto = *ctx;
        p.contexto.user_rip = p.contexto.user_rip.saturating_sub(2);
        p.estado = EstadoProceso::Listo;
    }

    tabla.lista[idx_sig].estado = EstadoProceso::Ejecutando;
    tabla.actual = tabla.lista[idx_sig].pid;
    tabla.lista[idx_sig].activar_hardware();
    *ctx = tabla.lista[idx_sig].contexto;
    true
}

/// Implementacion de `sched_yield()` (Linux syscall 24).
pub fn hacer_yield(ctx: &mut ContextoSyscall) {
    let mut tabla = PROCESOS.lock();
    let pid_actual = tabla.actual;
    let Some(idx_sig) = tabla
        .lista
        .iter()
        .position(|p| p.pid != pid_actual && p.estado == EstadoProceso::Listo)
    else {
        ctx.rax = 0;
        return;
    };

    if let Some(p) = tabla.lista.iter_mut().find(|p| p.pid == pid_actual) {
        p.contexto = *ctx;
        p.contexto.rax = 0;
        p.estado = EstadoProceso::Listo;
    }

    tabla.lista[idx_sig].estado = EstadoProceso::Ejecutando;
    tabla.actual = tabla.lista[idx_sig].pid;
    tabla.lista[idx_sig].activar_hardware();
    *ctx = tabla.lista[idx_sig].contexto;
}

/// Implementacion de `exit()` / `exit_group()`.
pub fn hacer_exit(codigo: i64, ctx: &mut ContextoSyscall) -> Option<i64> {
    let mut tabla = PROCESOS.lock();
    let mi_pid = tabla.actual;

    if mi_pid == 1 {
        memoria::activar_kernel_cr3();
        FsBase::write(VirtAddr::new(0));
        while let Some(p) = tabla.lista.pop() {
            p.espacio.liberar();
        }
        tabla.actual = 0;
        return Some(codigo);
    }

    let idx_yo = tabla.lista.iter().position(|p| p.pid == mi_pid)?;
    let mi_ppid = tabla.lista[idx_yo].ppid;
    // Cerramos de inmediato todos los FDs (y extremos de pipe!) antes de despertar a otros procesos.
    tabla.lista[idx_yo].fds = TablaFds {
        fds: [const { None }; 32],
    };
    tabla.lista[idx_yo].estado = EstadoProceso::Terminado(codigo);

    let padre_espera = tabla.lista.iter().position(|p| {
        p.pid == mi_ppid
            && matches!(
                p.estado,
                EstadoProceso::EsperandoHijo { pid_esperado, .. }
                    if coincide_espera(pid_esperado, mi_pid)
            )
    });

    if let Some(idx_padre) = padre_espera {
        let wstatus_ptr = match tabla.lista[idx_padre].estado {
            EstadoProceso::EsperandoHijo { wstatus_ptr, .. } => wstatus_ptr,
            _ => 0,
        };
        tabla.lista[idx_padre].activar_hardware();
        tabla.lista[idx_padre].estado = EstadoProceso::Ejecutando;
        tabla.actual = mi_ppid;
        *ctx = tabla.lista[idx_padre].contexto;
        ctx.rax = mi_pid;

        if let Some(pos_hijo) = tabla.lista.iter().position(|p| p.pid == mi_pid) {
            let hijo = tabla.lista.remove(pos_hijo);
            hijo.espacio.liberar();
        }

        escribir_wstatus(wstatus_ptr, codigo);
        return None;
    }

    let idx_sig = tabla
        .lista
        .iter()
        .position(|p| p.pid == mi_ppid && p.estado == EstadoProceso::Listo)
        .or_else(|| {
            tabla
                .lista
                .iter()
                .position(|p| p.estado == EstadoProceso::Listo)
        });

    if let Some(idx_sig) = idx_sig {
        tabla.lista[idx_sig].estado = EstadoProceso::Ejecutando;
        tabla.actual = tabla.lista[idx_sig].pid;
        tabla.lista[idx_sig].activar_hardware();
        *ctx = tabla.lista[idx_sig].contexto;
        None
    } else {
        println!("[neo_ws] Sin procesos listos tras salir PID {}", mi_pid);
        memoria::activar_kernel_cr3();
        FsBase::write(VirtAddr::new(0));
        Some(codigo)
    }
}

/// Genera el contenido dinamico de `/proc/ps`.
pub fn formatear_ps() -> String {
    let tabla = PROCESOS.lock();
    let mut salida = String::from("PID  PPID  ESTADO      COMANDO\n");
    for p in &tabla.lista {
        let est = match p.estado {
            EstadoProceso::Ejecutando => "EJECUTANDO ",
            EstadoProceso::Listo => "LISTO      ",
            EstadoProceso::EsperandoHijo { .. } => "ESPERANDO  ",
            EstadoProceso::Terminado(_) => "ZOMBIE     ",
        };
        salida.push_str(&format!("{:<4} {:<5} {} {}\n", p.pid, p.ppid, est, p.nombre));
    }
    salida
}
