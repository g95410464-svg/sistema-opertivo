// Capa de compatibilidad con Linux x86_64: 36 syscalls POSIX/Linux + SYSCALL/SYSRET.
use crate::fs::{self, Descriptor, StatLinux};
use crate::proceso::{self, ContextoSyscall};
use crate::{entrada, fb, gdt, interrupciones, print, println, teclado};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::arch::{asm, naked_asm};
use x86_64::registers::model_specific::{Efer, EferFlags, LStar, SFMask, Star};
use x86_64::registers::rflags::RFlags;
use x86_64::VirtAddr;

// Numeros oficiales de syscalls de Linux x86_64.
const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_STAT: u64 = 4;
const SYS_FSTAT: u64 = 5;
const SYS_LSTAT: u64 = 6;
const SYS_POLL: u64 = 7;
const SYS_LSEEK: u64 = 8;
const SYS_MMAP: u64 = 9;
const SYS_MPROTECT: u64 = 10;
const SYS_MUNMAP: u64 = 11;
const SYS_BRK: u64 = 12;
const SYS_RT_SIGACTION: u64 = 13;
const SYS_RT_SIGPROCMASK: u64 = 14;
const SYS_IOCTL: u64 = 16;
const SYS_READV: u64 = 19;
const SYS_WRITEV: u64 = 20;
const SYS_PIPE: u64 = 22;
const SYS_SCHED_YIELD: u64 = 24;
const SYS_MADVISE: u64 = 28;
const SYS_DUP: u64 = 32;
const SYS_DUP2: u64 = 33;
const SYS_NANOSLEEP: u64 = 35;
const SYS_GETPID: u64 = 39;
const SYS_FORK: u64 = 57;
const SYS_EXECVE: u64 = 59;
const SYS_EXIT: u64 = 60;
const SYS_WAIT4: u64 = 61;
const SYS_UNAME: u64 = 63;
const SYS_FCNTL: u64 = 72;
const SYS_GETCWD: u64 = 79;
const SYS_CHDIR: u64 = 80;
const SYS_GETUID: u64 = 102;
const SYS_GETGID: u64 = 104;
const SYS_GETEUID: u64 = 107;
const SYS_GETEGID: u64 = 108;
const SYS_GETPPID: u64 = 110;
const SYS_SIGALTSTACK: u64 = 131;
const SYS_ARCH_PRCTL: u64 = 158;
const SYS_GETTID: u64 = 186;
const SYS_TKILL: u64 = 200;
const SYS_SCHED_GETAFFINITY: u64 = 204;
const SYS_GETDENTS64: u64 = 217;
const SYS_SET_TID_ADDRESS: u64 = 218;
const SYS_CLOCK_GETTIME: u64 = 228;
const SYS_EXIT_GROUP: u64 = 231;
const SYS_OPENAT: u64 = 257;
const SYS_NEWFSTATAT: u64 = 262;
const SYS_PIPE2: u64 = 293;
const SYS_PRLIMIT64: u64 = 302;
const SYS_GETRANDOM: u64 = 318;

// Codigos de error POSIX/Linux (negativos).
const EBADF: i64 = -9;
const EFAULT: i64 = -14;
const EINVAL: i64 = -22;
const ENOTTY: i64 = -25;
const ERANGE: i64 = -34;
const ENOSYS: i64 = -38;

const TIOCGWINSZ: u64 = 0x5413;
const FBIOGET_VSCREENINFO: u64 = 0x4600;
const FBIOGET_FSCREENINFO: u64 = 0x4602;
const FBIO_DESACTIVAR: u64 = 0x46F0;
const LIMITE_USUARIO: u64 = 0x0000_8000_0000_0000;

#[repr(C, align(16))]
struct PilaSyscall([u8; 4096 * 16]);

static mut PILA_SYSCALL: PilaSyscall = PilaSyscall([0; 4096 * 16]);
static mut KERNEL_SYSCALL_RSP: u64 = 0;
static mut USER_RSP_TMP: u64 = 0;
static mut RETORNO_KERNEL_RSP: u64 = 0;
static mut CODIGO_SALIDA: i64 = 0;

#[repr(C)]
#[derive(Clone, Copy)]
struct IoVec {
    base: u64,
    len: u64,
}

#[repr(C)]
struct WinSize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

#[repr(C)]
struct TimeSpec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

/// Configura los registros MSR del CPU para habilitar la instruccion `syscall`.
pub fn init() {
    let sel = gdt::selectores();
    unsafe {
        let tope = (&raw const PILA_SYSCALL as u64) + core::mem::size_of::<PilaSyscall>() as u64;
        KERNEL_SYSCALL_RSP = tope;

        Efer::update(|flags| flags.insert(EferFlags::SYSTEM_CALL_EXTENSIONS));

        Star::write(
            sel.codigo_usuario,
            sel.datos_usuario,
            sel.codigo_kernel,
            sel.datos_kernel,
        )
        .expect("Selectores GDT invalidos para STAR");

        LStar::write(VirtAddr::new(entrada_syscall as *const () as u64));
        SFMask::write(RFlags::INTERRUPT_FLAG);
    }
}

/// Entra a Ring 3 en el proceso inicial (cuyo CR3 ya esta activo) y devuelve el codigo al salir.
pub fn ejecutar_en_usuario(punto_entrada: u64, pila_usuario: u64) -> i64 {
    let sel = gdt::selectores();
    unsafe {
        entrar_usuario(
            punto_entrada,
            pila_usuario,
            sel.codigo_usuario.0 as u64,
            sel.datos_usuario.0 as u64,
        )
    }
}

#[unsafe(naked)]
unsafe extern "sysv64" fn entrar_usuario(
    _entry: u64,
    _user_rsp: u64,
    _cs_user: u64,
    _ss_user: u64,
) -> i64 {
    naked_asm!(
        "pushfq",
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "mov [rip + {retorno}], rsp",

        "push rcx",
        "push rsi",
        "push 0x202",
        "push rdx",
        "push rdi",

        "xor rax, rax",
        "xor rbx, rbx",
        "xor rcx, rcx",
        "xor rdx, rdx",
        "xor rsi, rsi",
        "xor rdi, rdi",
        "xor rbp, rbp",
        "xor r8, r8",
        "xor r9, r9",
        "xor r10, r10",
        "xor r11, r11",
        "xor r12, r12",
        "xor r13, r13",
        "xor r14, r14",
        "xor r15, r15",
        "iretq",
        retorno = sym RETORNO_KERNEL_RSP,
    );
}

unsafe fn salir_de_usuario(codigo: i64) -> ! {
    CODIGO_SALIDA = codigo;
    asm!(
        "mov rsp, [rip + {retorno}]",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",
        "popfq",
        "mov rax, [rip + {codigo}]",
        "ret",
        retorno = sym RETORNO_KERNEL_RSP,
        codigo = sym CODIGO_SALIDA,
        options(noreturn)
    );
}

#[unsafe(naked)]
unsafe extern "C" fn entrada_syscall() {
    naked_asm!(
        "mov [rip + {user_rsp}], rsp",
        "mov rsp, [rip + {kernel_rsp}]",

        "push qword ptr [rip + {user_rsp}]",
        "push r11",
        "push rcx",
        "push r15",
        "push r14",
        "push r13",
        "push r12",
        "push rbp",
        "push rbx",
        "push r9",
        "push r8",
        "push r10",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rax",

        "mov rdi, rsp",
        "call {despachar}",

        "pop rax",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop r10",
        "pop r8",
        "pop r9",
        "pop rbx",
        "pop rbp",
        "pop r12",
        "pop r13",
        "pop r14",
        "pop r15",
        "pop rcx",
        "pop r11",
        "pop rsp",
        "sysretq",
        user_rsp = sym USER_RSP_TMP,
        kernel_rsp = sym KERNEL_SYSCALL_RSP,
        despachar = sym despachar_syscall,
    );
}

fn validar_rango_usuario(ptr: u64, len: u64) -> bool {
    ptr >= 0x1000 && len <= 16 * 1024 * 1024 && ptr.saturating_add(len) < LIMITE_USUARIO
}

fn copiar_cstr_usuario(ptr: u64) -> Option<String> {
    if !validar_rango_usuario(ptr, 1) {
        return None;
    }
    let mut len = 0usize;
    while len < 256 {
        let dir = ptr + len as u64;
        if dir >= LIMITE_USUARIO {
            return None;
        }
        let b = unsafe { *(dir as *const u8) };
        if b == 0 {
            break;
        }
        len += 1;
    }
    let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
    core::str::from_utf8(bytes).ok().map(|s| s.to_string())
}

extern "sysv64" fn despachar_syscall(ctx: &mut ContextoSyscall) {
    match ctx.rax {
        SYS_READ => {
            if let Some(n) = sys_read(ctx.rdi as usize, ctx.rsi, ctx.rdx, ctx) {
                ctx.rax = n as u64;
            }
        }
        SYS_WRITE => ctx.rax = sys_write(ctx.rdi as usize, ctx.rsi, ctx.rdx) as u64,
        SYS_READV => ctx.rax = sys_readv(ctx.rdi as usize, ctx.rsi, ctx.rdx) as u64,
        SYS_WRITEV => ctx.rax = sys_writev(ctx.rdi as usize, ctx.rsi, ctx.rdx) as u64,
        SYS_OPEN => ctx.rax = sys_openat(ctx.rdi, ctx.rsi) as u64,
        SYS_OPENAT => ctx.rax = sys_openat(ctx.rsi, ctx.rdx) as u64,
        SYS_CLOSE => ctx.rax = proceso::cerrar_fd(ctx.rdi as usize) as u64,
        SYS_STAT | SYS_LSTAT => ctx.rax = sys_stat(ctx.rdi, ctx.rsi) as u64,
        SYS_FSTAT => ctx.rax = sys_fstat(ctx.rdi as usize, ctx.rsi) as u64,
        SYS_POLL => ctx.rax = sys_poll(ctx.rdi, ctx.rsi, ctx.rdx as i32) as u64,
        SYS_NEWFSTATAT => ctx.rax = sys_stat(ctx.rsi, ctx.rdx) as u64,
        SYS_LSEEK => ctx.rax = sys_lseek(ctx.rdi as usize, ctx.rsi as i64, ctx.rdx) as u64,
        SYS_MMAP => {
            ctx.rax = proceso::hacer_mmap(ctx.rdi, ctx.rsi, ctx.rdx, ctx.r10, ctx.r8 as i64) as u64
        }
        SYS_MPROTECT | SYS_MUNMAP | SYS_MADVISE | SYS_SIGALTSTACK => ctx.rax = 0,
        SYS_BRK => ctx.rax = proceso::hacer_brk(ctx.rdi),
        SYS_RT_SIGACTION | SYS_RT_SIGPROCMASK => ctx.rax = 0,
        SYS_IOCTL => ctx.rax = sys_ioctl(ctx.rdi as usize, ctx.rsi, ctx.rdx) as u64,
        SYS_PIPE | SYS_PIPE2 => ctx.rax = sys_pipe(ctx.rdi) as u64,
        SYS_SCHED_YIELD => proceso::hacer_yield(ctx),
        SYS_DUP => ctx.rax = proceso::hacer_dup(ctx.rdi as usize) as u64,
        SYS_DUP2 => ctx.rax = proceso::hacer_dup2(ctx.rdi as usize, ctx.rsi as usize) as u64,
        SYS_NANOSLEEP => ctx.rax = sys_nanosleep(ctx.rdi) as u64,
        SYS_GETPID | SYS_GETTID | SYS_SET_TID_ADDRESS => ctx.rax = proceso::pid_actual(),
        SYS_GETPPID => ctx.rax = proceso::ppid_actual(),
        SYS_GETUID | SYS_GETGID | SYS_GETEUID | SYS_GETEGID => ctx.rax = 0,
        SYS_SCHED_GETAFFINITY => ctx.rax = sys_sched_getaffinity(ctx.rsi, ctx.rdx) as u64,
        SYS_TKILL => {
            // Si un proceso se envia una senal fatal (ej. SIGABRT=6), terminamos ese proceso limpiamente.
            if let Some(codigo_init) = proceso::hacer_exit(128 + (ctx.rsi as i64), ctx) {
                unsafe { salir_de_usuario(codigo_init) };
            }
        }
        SYS_FORK => ctx.rax = proceso::hacer_fork(ctx) as u64,
        SYS_EXECVE => {
            let err = sys_execve(ctx.rdi, ctx.rsi, ctx);
            if err < 0 {
                ctx.rax = err as u64;
            }
        }
        SYS_WAIT4 => proceso::hacer_wait4(ctx.rdi as i64, ctx.rsi, ctx),
        SYS_UNAME => ctx.rax = sys_uname(ctx.rdi) as u64,
        SYS_FCNTL => ctx.rax = sys_fcntl(ctx.rdi as usize, ctx.rsi, ctx.rdx) as u64,
        SYS_GETCWD => ctx.rax = sys_getcwd(ctx.rdi, ctx.rsi) as u64,
        SYS_CHDIR => ctx.rax = sys_chdir(ctx.rdi) as u64,
        SYS_ARCH_PRCTL => ctx.rax = proceso::hacer_arch_prctl(ctx.rdi, ctx.rsi) as u64,
        SYS_GETDENTS64 => ctx.rax = sys_getdents64(ctx.rdi as usize, ctx.rsi, ctx.rdx) as u64,
        SYS_CLOCK_GETTIME => ctx.rax = sys_clock_gettime(ctx.rsi) as u64,
        SYS_PRLIMIT64 => ctx.rax = 0,
        SYS_GETRANDOM => ctx.rax = sys_getrandom(ctx.rdi, ctx.rsi) as u64,
        SYS_EXIT | SYS_EXIT_GROUP => {
            if let Some(codigo_init) = proceso::hacer_exit(ctx.rdi as i64, ctx) {
                unsafe { salir_de_usuario(codigo_init) };
            }
        }
        otra => {
            println!("[neo_ws] Syscall Linux no implementada: {}", otra);
            ctx.rax = ENOSYS as u64;
        }
    }
}

/// Devuelve `None` si el proceso cedio CPU para reintentar lectura bloqueante de un pipe.
fn sys_read(fd: usize, buf: u64, len: u64, ctx: &mut ContextoSyscall) -> Option<i64> {
    if len == 0 {
        return Some(0);
    }
    if !validar_rango_usuario(buf, len) {
        return Some(EFAULT);
    }
    let Some(desc) = proceso::obtener_descriptor(fd) else {
        return Some(EBADF);
    };
    let dst = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, len as usize) };
    match desc {
        Descriptor::TtyEntrada => Some(teclado::leer_linea(dst) as i64),
        Descriptor::DevInputEvents => Some(entrada::leer_eventos(dst) as i64),
        Descriptor::TtySalida | Descriptor::PipeEscritura(_) | Descriptor::DevFb0 => Some(EBADF),
        Descriptor::Archivo(est) => Some(fs::leer_archivo(&est, dst)),
        Descriptor::PipeLectura(pipe) => match fs::leer_pipe(&pipe, dst) {
            Ok(n) => Some(n),
            Err(()) => {
                if proceso::ceder_y_reintentar(ctx) {
                    None
                } else {
                    Some(0)
                }
            }
        },
    }
}

fn escribir_en_descriptor(desc: &Descriptor, bytes: &[u8]) -> i64 {
    match desc {
        Descriptor::TtySalida => {
            let texto = core::str::from_utf8(bytes).unwrap_or("<bytes no UTF-8>");
            print!("{}", texto);
            bytes.len() as i64
        }
        Descriptor::TtyEntrada | Descriptor::PipeLectura(_) | Descriptor::DevInputEvents | Descriptor::DevFb0 => EBADF,
        Descriptor::Archivo(est) => fs::escribir_archivo(est, bytes),
        Descriptor::PipeEscritura(pipe) => fs::escribir_pipe(pipe, bytes),
    }
}

fn sys_write(fd: usize, buf: u64, len: u64) -> i64 {
    if len == 0 {
        return 0;
    }
    if !validar_rango_usuario(buf, len) {
        return EFAULT;
    }
    let Some(desc) = proceso::obtener_descriptor(fd) else {
        return EBADF;
    };
    let bytes = unsafe { core::slice::from_raw_parts(buf as *const u8, len as usize) };
    escribir_en_descriptor(&desc, bytes)
}

fn sys_writev(fd: usize, iov_ptr: u64, iovcnt: u64) -> i64 {
    if iovcnt == 0 {
        return 0;
    }
    if iovcnt > 64 || !validar_rango_usuario(iov_ptr, iovcnt * core::mem::size_of::<IoVec>() as u64) {
        return EFAULT;
    }
    let Some(desc) = proceso::obtener_descriptor(fd) else {
        return EBADF;
    };
    let iovs = unsafe { core::slice::from_raw_parts(iov_ptr as *const IoVec, iovcnt as usize) };
    let mut total: i64 = 0;
    for v in iovs {
        if v.len == 0 {
            continue;
        }
        if !validar_rango_usuario(v.base, v.len) {
            return EFAULT;
        }
        let bytes = unsafe { core::slice::from_raw_parts(v.base as *const u8, v.len as usize) };
        let n = escribir_en_descriptor(&desc, bytes);
        if n < 0 {
            return if total > 0 { total } else { n };
        }
        total += n;
    }
    total
}

fn sys_readv(fd: usize, iov_ptr: u64, iovcnt: u64) -> i64 {
    if iovcnt == 0 {
        return 0;
    }
    if iovcnt > 64 || !validar_rango_usuario(iov_ptr, iovcnt * core::mem::size_of::<IoVec>() as u64) {
        return EFAULT;
    }
    let Some(desc) = proceso::obtener_descriptor(fd) else {
        return EBADF;
    };
    let iovs = unsafe { core::slice::from_raw_parts(iov_ptr as *const IoVec, iovcnt as usize) };
    let mut total: i64 = 0;
    for v in iovs {
        if v.len == 0 {
            continue;
        }
        if !validar_rango_usuario(v.base, v.len) {
            return EFAULT;
        }
        let dst = unsafe { core::slice::from_raw_parts_mut(v.base as *mut u8, v.len as usize) };
        let n = match &desc {
            Descriptor::TtyEntrada => teclado::leer_linea(dst) as i64,
            Descriptor::Archivo(est) => fs::leer_archivo(est, dst),
            Descriptor::PipeLectura(pipe) => fs::leer_pipe(pipe, dst).unwrap_or(0),
            _ => return EBADF,
        };
        if n <= 0 {
            return if total > 0 { total } else { n };
        }
        total += n;
        if (n as u64) < v.len {
            break;
        }
    }
    total
}

fn sys_openat(ruta_ptr: u64, flags: u64) -> i64 {
    let Some(ruta) = copiar_cstr_usuario(ruta_ptr) else {
        return EFAULT;
    };
    proceso::abrir_fd(&ruta, flags)
}

fn sys_stat(ruta_ptr: u64, stat_ptr: u64) -> i64 {
    if !validar_rango_usuario(stat_ptr, core::mem::size_of::<StatLinux>() as u64) {
        return EFAULT;
    }
    let Some(ruta) = copiar_cstr_usuario(ruta_ptr) else {
        return EFAULT;
    };
    let cwd = proceso::cwd_actual();
    match fs::stat_ruta(&cwd, &ruta) {
        Ok(st) => {
            unsafe { (stat_ptr as *mut StatLinux).write(st) };
            0
        }
        Err(e) => e,
    }
}

fn sys_fstat(fd: usize, stat_ptr: u64) -> i64 {
    if !validar_rango_usuario(stat_ptr, core::mem::size_of::<StatLinux>() as u64) {
        return EFAULT;
    }
    let Some(desc) = proceso::obtener_descriptor(fd) else {
        return EBADF;
    };
    match fs::stat_descriptor(&desc) {
        Ok(st) => {
            unsafe { (stat_ptr as *mut StatLinux).write(st) };
            0
        }
        Err(e) => e,
    }
}

fn sys_lseek(fd: usize, offset: i64, whence: u64) -> i64 {
    let Some(desc) = proceso::obtener_descriptor(fd) else {
        return EBADF;
    };
    fs::lseek_descriptor(&desc, offset, whence)
}

fn sys_ioctl(fd: usize, req: u64, arg: u64) -> i64 {
    let Some(desc) = proceso::obtener_descriptor(fd) else {
        return EBADF;
    };
    match desc {
        Descriptor::TtyEntrada | Descriptor::TtySalida => {
            if req == TIOCGWINSZ {
                if !validar_rango_usuario(arg, core::mem::size_of::<WinSize>() as u64) {
                    return EFAULT;
                }
                let ws = WinSize {
                    ws_row: 25,
                    ws_col: 80,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                unsafe { (arg as *mut WinSize).write(ws) };
                0
            } else {
                0
            }
        }
        Descriptor::DevFb0 => {
            let Some(info) = fb::info() else {
                return EINVAL;
            };
            match req {
                FBIOGET_VSCREENINFO => {
                    if !validar_rango_usuario(arg, 160) {
                        return EFAULT;
                    }
                    let mut vinfo = [0u32; 40];
                    vinfo[0] = info.ancho;
                    vinfo[1] = info.alto;
                    vinfo[2] = info.ancho;
                    vinfo[3] = info.alto;
                    vinfo[6] = info.bpp;
                    vinfo[9] = 16; // red.offset
                    vinfo[10] = 8; // red.length
                    vinfo[12] = 8; // green.offset
                    vinfo[13] = 8; // green.length
                    vinfo[15] = 0; // blue.offset
                    vinfo[16] = 8; // blue.length
                    let dst = unsafe { core::slice::from_raw_parts_mut(arg as *mut u32, 40) };
                    dst.copy_from_slice(&vinfo);
                    0
                }
                FBIOGET_FSCREENINFO => {
                    if !validar_rango_usuario(arg, 68) {
                        return EFAULT;
                    }
                    let dst = unsafe { core::slice::from_raw_parts_mut(arg as *mut u8, 68) };
                    dst.fill(0);
                    let nombre = b"neo_ws_bga";
                    dst[..nombre.len()].copy_from_slice(nombre);
                    dst[24..28].copy_from_slice(&(info.tamano as u32).to_le_bytes());
                    dst[36..40].copy_from_slice(&2u32.to_le_bytes()); // FB_VISUAL_TRUECOLOR
                    dst[44..48].copy_from_slice(&info.bytes_linea.to_le_bytes());
                    0
                }
                FBIO_DESACTIVAR => {
                    fb::desactivar_modo_grafico();
                    0
                }
                _ => EINVAL,
            }
        }
        _ => ENOTTY,
    }
}

fn sys_pipe(fds_ptr: u64) -> i64 {
    if !validar_rango_usuario(fds_ptr, 8) {
        return EFAULT;
    }
    match proceso::hacer_pipe() {
        Ok((r, w)) => {
            let out = unsafe { core::slice::from_raw_parts_mut(fds_ptr as *mut i32, 2) };
            out[0] = r as i32;
            out[1] = w as i32;
            0
        }
        Err(e) => e,
    }
}

fn sys_fcntl(fd: usize, cmd: u64, arg: u64) -> i64 {
    match cmd {
        0 | 1030 => proceso::hacer_dup_desde(fd, arg as usize), // F_DUPFD / F_DUPFD_CLOEXEC
        1 | 2 | 3 | 4 => 0, // F_GETFD, F_SETFD, F_GETFL, F_SETFL
        _ => EINVAL,
    }
}

fn sys_getcwd(buf: u64, size: u64) -> i64 {
    if size == 0 || !validar_rango_usuario(buf, size) {
        return EFAULT;
    }
    let cwd = proceso::cwd_actual();
    let b = cwd.as_bytes();
    if (b.len() as u64) + 1 > size {
        return ERANGE;
    }
    let dst = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, size as usize) };
    dst[..b.len()].copy_from_slice(b);
    dst[b.len()] = 0;
    (b.len() + 1) as i64
}

fn sys_chdir(ruta_ptr: u64) -> i64 {
    let Some(ruta) = copiar_cstr_usuario(ruta_ptr) else {
        return EFAULT;
    };
    proceso::hacer_chdir(&ruta)
}

fn sys_execve(ruta_ptr: u64, argv_ptr: u64, ctx: &mut ContextoSyscall) -> i64 {
    let Some(ruta) = copiar_cstr_usuario(ruta_ptr) else {
        return EFAULT;
    };
    let mut argv: Vec<String> = Vec::new();
    if argv_ptr != 0 {
        for i in 0..16u64 {
            let entry_addr = argv_ptr + i * 8;
            if !validar_rango_usuario(entry_addr, 8) {
                return EFAULT;
            }
            let str_ptr = unsafe { *(entry_addr as *const u64) };
            if str_ptr == 0 {
                break;
            }
            let Some(arg) = copiar_cstr_usuario(str_ptr) else {
                return EFAULT;
            };
            argv.push(arg);
        }
    }
    proceso::hacer_execve(&ruta, &argv, ctx)
}

fn sys_getdents64(fd: usize, buf: u64, len: u64) -> i64 {
    if len == 0 {
        return 0;
    }
    if !validar_rango_usuario(buf, len) {
        return EFAULT;
    }
    let Some(Descriptor::Archivo(est)) = proceso::obtener_descriptor(fd) else {
        return EBADF;
    };
    let dst = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, len as usize) };
    fs::getdents64_archivo(&est, dst)
}

fn sys_poll(fds_ptr: u64, nfds: u64, timeout_ms: i32) -> i64 {
    use x86_64::instructions::interrupts;
    const POLLNVAL: i16 = 0x20;
    const POLLIN: i16 = 0x01;
    const POLLOUT: i16 = 0x04;
    if nfds == 0 {
        return 0;
    }
    if nfds > 64 || !validar_rango_usuario(fds_ptr, nfds * core::mem::size_of::<PollFd>() as u64) {
        return EFAULT;
    }
    let fds = unsafe { core::slice::from_raw_parts_mut(fds_ptr as *mut PollFd, nfds as usize) };
    let limite_ticks = if timeout_ms > 0 {
        let espera = ((timeout_ms as u64) + interrupciones::MS_POR_TICK - 1)
            / interrupciones::MS_POR_TICK;
        interrupciones::ticks().saturating_add(espera.max(1))
    } else {
        interrupciones::ticks()
    };

    loop {
        let mut listos: i64 = 0;
        for pfd in fds.iter_mut() {
            pfd.revents = 0;
            if pfd.fd < 0 {
                continue;
            }
            match proceso::obtener_descriptor(pfd.fd as usize) {
                None => {
                    pfd.revents = POLLNVAL;
                    listos += 1;
                }
                Some(Descriptor::DevInputEvents) => {
                    if (pfd.events & POLLIN) != 0 && crate::entrada::hay_eventos() {
                        pfd.revents |= POLLIN;
                        listos += 1;
                    }
                }
                Some(_) => {
                    if (pfd.events & POLLOUT) != 0 {
                        pfd.revents |= POLLOUT;
                    }
                    if (pfd.events & POLLIN) != 0 {
                        pfd.revents |= POLLIN;
                    }
                    if pfd.revents != 0 {
                        listos += 1;
                    }
                }
            }
        }
        if listos > 0 || timeout_ms == 0 || (timeout_ms > 0 && interrupciones::ticks() >= limite_ticks) {
            return listos;
        }
        // Despierta en microsegundos ante CUALQUIER IRQ (raton, teclado o timer 250 Hz).
        interrupts::enable_and_hlt();
        interrupts::disable();
    }
}

fn sys_nanosleep(req_ptr: u64) -> i64 {
    use x86_64::instructions::interrupts;
    if !validar_rango_usuario(req_ptr, core::mem::size_of::<TimeSpec>() as u64) {
        return EFAULT;
    }
    let req = unsafe { core::ptr::read_unaligned(req_ptr as *const TimeSpec) };
    if req.tv_sec < 0 || req.tv_nsec < 0 {
        return EINVAL;
    }
    let total_ms = (req.tv_sec as u64).saturating_mul(1000) + (req.tv_nsec as u64) / 1_000_000;
    let ticks_espera = if total_ms == 0 {
        0
    } else {
        (total_ms + interrupciones::MS_POR_TICK - 1) / interrupciones::MS_POR_TICK
    };
    let objetivo = interrupciones::ticks().saturating_add(ticks_espera);
    while interrupciones::ticks() < objetivo {
        interrupts::enable_and_hlt();
        interrupts::disable();
    }
    0
}

fn sys_sched_getaffinity(size: u64, mask_ptr: u64) -> i64 {
    if size == 0 || !validar_rango_usuario(mask_ptr, size) {
        return EFAULT;
    }
    let dst = unsafe { core::slice::from_raw_parts_mut(mask_ptr as *mut u8, size as usize) };
    dst.fill(0);
    dst[0] = 1; // 1 CPU activo (CPU 0)
    size.min(8) as i64
}

fn sys_clock_gettime(ts_ptr: u64) -> i64 {
    if !validar_rango_usuario(ts_ptr, core::mem::size_of::<TimeSpec>() as u64) {
        return EFAULT;
    }
    let ms = interrupciones::ms_transcurridos();
    let ts = TimeSpec {
        tv_sec: (ms / 1000) as i64,
        tv_nsec: ((ms % 1000) * 1_000_000) as i64,
    };
    unsafe { (ts_ptr as *mut TimeSpec).write(ts) };
    0
}

fn sys_getrandom(buf: u64, len: u64) -> i64 {
    if len == 0 {
        return 0;
    }
    if !validar_rango_usuario(buf, len) {
        return EFAULT;
    }
    let dst = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, len as usize) };
    let mut estado = interrupciones::ticks().wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0x243f_6a88_85a3_08d3;
    for b in dst.iter_mut() {
        estado ^= estado << 13;
        estado ^= estado >> 7;
        estado ^= estado << 17;
        *b = estado as u8;
    }
    len as i64
}

fn sys_uname(buf: u64) -> i64 {
    const TAMANO_UTSNAME: u64 = 65 * 6;
    if !validar_rango_usuario(buf, TAMANO_UTSNAME) {
        return EFAULT;
    }
    let dst = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, TAMANO_UTSNAME as usize) };
    dst.fill(0);

    let campos: [&[u8]; 6] = [
        b"Linux",
        b"neo_ws",
        b"6.1.0-neo_ws",
        b"#4 neo_ws Rust Framekernel (GUI + musl)",
        b"x86_64",
        b"(none)",
    ];
    for (i, texto) in campos.iter().enumerate() {
        let base = i * 65;
        let n = texto.len().min(64);
        dst[base..base + n].copy_from_slice(&texto[..n]);
    }
    0
}
