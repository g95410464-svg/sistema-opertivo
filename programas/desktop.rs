// neo_ws 1.0 — Entorno de escritorio suave, minimalista y completo en Ring 3 (Linux x86_64 ELF64).
// Incluye pantalla de Login, motor grafico anti-aliased (esquinas redondeadas + fuentes suaves),
// barra superior de 3 capsulas (workspaces, calendario, centro de control), selector de 4 fondos,
// dock flotante, Terminal con fastfetch y binarios musl/std, btop Monitor, Editor/Visor de codigo y Paint.
// Compilar con:
//   rustc +nightly --target x86_64-unknown-none -O -C panic=abort -C strip=symbols -C relocation-model=static -C link-arg=--image-base=0x400000 programas/desktop.rs -o programas/desktop.elf
#![no_std]
#![no_main]

mod doom3d;
mod fuentes_suaves;
mod gfx;
mod mascota;
mod web;

use core::arch::asm;
use core::panic::PanicInfo;
use fuentes_suaves::{MONO_H, MONO_W};
use gfx::{dibujar_cursor_en, generar_blur, generar_wallpaper, mezclar, restaurar_cursor_fb, Lienzo, H, W};
use mascota::{MASCOTA, MASCOTA_H, MASCOTA_W};

// Syscalls oficiales de Linux x86_64
const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_CLOSE: u64 = 3;
const SYS_POLL: u64 = 7;
const SYS_MMAP: u64 = 9;
const SYS_IOCTL: u64 = 16;
const SYS_PIPE: u64 = 22;
const SYS_DUP2: u64 = 33;
const SYS_NANOSLEEP: u64 = 35;
const SYS_FORK: u64 = 57;
const SYS_EXECVE: u64 = 59;
const SYS_EXIT: u64 = 60;
const SYS_WAIT4: u64 = 61;
const SYS_UNAME: u64 = 63;
const SYS_GETCWD: u64 = 79;
const SYS_CHDIR: u64 = 80;
const SYS_GETDENTS64: u64 = 217;
const SYS_CLOCK_GETTIME: u64 = 228;
const SYS_OPENAT: u64 = 257;

const AT_FDCWD: i64 = -100;
const O_RDONLY: u64 = 0;
const O_WRONLY: u64 = 1;
const O_RDWR: u64 = 2;
const O_CREAT: u64 = 0x40;
const O_TRUNC: u64 = 0x200;

const PROT_READ: u64 = 1;
const PROT_WRITE: u64 = 2;
const MAP_SHARED: u64 = 0x01;
const MAP_PRIVATE: u64 = 0x02;
const MAP_ANONYMOUS: u64 = 0x20;

const FBIOGET_VSCREENINFO: u64 = 0x4600;
const FBIO_DESACTIVAR: u64 = 0x46F0;

const TIPO_TECLA_ABAJO: u16 = 1;
const TIPO_RATON: u16 = 3;
const TECLA_ARRIBA: u16 = 0x100;
const TECLA_ABAJO: u16 = 0x101;
const TECLA_IZQ: u16 = 0x102;
const TECLA_DER: u16 = 0x103;
const TECLA_SUPR: u16 = 0x104;
const TECLA_INICIO: u16 = 0x105;
const TECLA_FIN: u16 = 0x106;
const CTRL_C: u16 = 3;
const CTRL_S: u16 = 19;
const CTRL_V: u16 = 22;
const DT_DIR: u8 = 4;

// Paleta suave moderna
const COL_TEXTO: u32 = 0xF1F5F9;
const COL_TEXTO_SUAVE: u32 = 0xA8B3C5;
const COL_TEXTO_DIM: u32 = 0x6E7A8E;
const COL_VERDE: u32 = 0x4ADE80;
const COL_CIAN: u32 = 0x38BDF8;
const COL_AMARILLO: u32 = 0xFACC15;
const COL_ROJO: u32 = 0xF87171;
const COL_PURPURA: u32 = 0xC084FC;
const COL_AZUL_ACENTO: u32 = 0x3B82F6;

#[repr(C)]
#[derive(Clone, Copy)]
struct EventoEntrada {
    tipo: u16,
    codigo: u16,
    x: i32,
    y: i32,
    botones: u32,
}

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

#[repr(C)]
struct TimeSpec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[repr(C)]
struct UtsName {
    sysname: [u8; 65],
    nodename: [u8; 65],
    release: [u8; 65],
    version: [u8; 65],
    machine: [u8; 65],
    domainname: [u8; 65],
}

// --- Envoltorios de syscalls ---

#[inline(always)]
unsafe fn sc0(n: u64) -> isize {
    let r: isize;
    asm!("syscall", in("rax") n, lateout("rax") r, out("rcx") _, out("r11") _);
    r
}
#[inline(always)]
unsafe fn sc1(n: u64, a1: u64) -> isize {
    let r: isize;
    asm!("syscall", in("rax") n, in("rdi") a1, lateout("rax") r, out("rcx") _, out("r11") _);
    r
}
#[inline(always)]
unsafe fn sc2(n: u64, a1: u64, a2: u64) -> isize {
    let r: isize;
    asm!("syscall", in("rax") n, in("rdi") a1, in("rsi") a2, lateout("rax") r, out("rcx") _, out("r11") _);
    r
}
#[inline(always)]
unsafe fn sc3(n: u64, a1: u64, a2: u64, a3: u64) -> isize {
    let r: isize;
    asm!("syscall", in("rax") n, in("rdi") a1, in("rsi") a2, in("rdx") a3, lateout("rax") r, out("rcx") _, out("r11") _);
    r
}
#[inline(always)]
unsafe fn sc4(n: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> isize {
    let r: isize;
    asm!("syscall", in("rax") n, in("rdi") a1, in("rsi") a2, in("rdx") a3, in("r10") a4, lateout("rax") r, out("rcx") _, out("r11") _);
    r
}
#[inline(always)]
unsafe fn sc6(n: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64) -> isize {
    let r: isize;
    asm!(
        "syscall",
        in("rax") n, in("rdi") a1, in("rsi") a2, in("rdx") a3,
        in("r10") a4, in("r8") a5, in("r9") a6,
        lateout("rax") r, out("rcx") _, out("r11") _
    );
    r
}

fn sys_open(ruta_cstr: &[u8], flags: u64) -> isize {
    unsafe { sc4(SYS_OPENAT, AT_FDCWD as u64, ruta_cstr.as_ptr() as u64, flags, 0o644) }
}
fn sys_close(fd: u64) {
    unsafe { sc1(SYS_CLOSE, fd); }
}
fn sys_read(fd: u64, buf: &mut [u8]) -> isize {
    unsafe { sc3(SYS_READ, fd, buf.as_mut_ptr() as u64, buf.len() as u64) }
}
fn sys_write(fd: u64, buf: &[u8]) -> isize {
    unsafe { sc3(SYS_WRITE, fd, buf.as_ptr() as u64, buf.len() as u64) }
}
fn sys_ioctl(fd: u64, req: u64, arg: u64) -> isize {
    unsafe { sc3(SYS_IOCTL, fd, req, arg) }
}
fn sys_mmap(len: usize, flags: u64, fd: i64) -> *mut u32 {
    let r = unsafe { sc6(SYS_MMAP, 0, len as u64, PROT_READ | PROT_WRITE, flags, fd as u64, 0) };
    if r < 0 { core::ptr::null_mut() } else { r as *mut u32 }
}
fn sys_pipe(fds: &mut [i32; 2]) -> isize {
    unsafe { sc1(SYS_PIPE, fds.as_mut_ptr() as u64) }
}
fn sys_dup2(v: u64, n: u64) -> isize {
    unsafe { sc2(SYS_DUP2, v, n) }
}
fn sys_fork() -> isize {
    unsafe { sc0(SYS_FORK) }
}
fn sys_execve(ruta: &[u8], argv: &[*const u8]) -> isize {
    let envp: [*const u8; 1] = [core::ptr::null()];
    unsafe { sc3(SYS_EXECVE, ruta.as_ptr() as u64, argv.as_ptr() as u64, envp.as_ptr() as u64) }
}
fn sys_wait4(pid: i64, st: &mut i32) -> isize {
    unsafe { sc4(SYS_WAIT4, pid as u64, st as *mut i32 as u64, 0, 0) }
}
fn sys_getcwd(buf: &mut [u8]) -> isize {
    unsafe { sc2(SYS_GETCWD, buf.as_mut_ptr() as u64, buf.len() as u64) }
}
fn sys_chdir(ruta: &[u8]) -> isize {
    unsafe { sc1(SYS_CHDIR, ruta.as_ptr() as u64) }
}
fn sys_getdents64(fd: u64, buf: &mut [u8]) -> isize {
    unsafe { sc3(SYS_GETDENTS64, fd, buf.as_mut_ptr() as u64, buf.len() as u64) }
}
fn sys_uname(u: &mut UtsName) -> isize {
    unsafe { sc1(SYS_UNAME, u as *mut UtsName as u64) }
}
fn sys_uptime_sec() -> u64 {
    let mut ts = TimeSpec { tv_sec: 0, tv_nsec: 0 };
    unsafe { sc2(SYS_CLOCK_GETTIME, 1, &mut ts as *mut TimeSpec as u64); }
    ts.tv_sec.max(0) as u64
}
fn sys_sleep_ms(ms: u64) {
    let ts = TimeSpec {
        tv_sec: (ms / 1000) as i64,
        tv_nsec: ((ms % 1000) * 1_000_000) as i64,
    };
    unsafe { sc2(SYS_NANOSLEEP, &ts as *const TimeSpec as u64, 0); }
}
fn sys_poll_entrada(ev_fd: i32, timeout_ms: i32) -> isize {
    let mut pfd = PollFd {
        fd: ev_fd,
        events: 0x01, // POLLIN
        revents: 0,
    };
    unsafe { sc3(SYS_POLL, &mut pfd as *mut PollFd as u64, 1, timeout_ms as u64) }
}

fn sys_exit(code: u64) -> ! {
    unsafe { asm!("syscall", in("rax") SYS_EXIT, in("rdi") code, options(noreturn)); }
}

fn leer_foto_wallpaper(dst: &mut [u8]) -> usize {
    let fd = sys_open(b"/usr/share/wallpaper.rgb\0", O_RDONLY);
    if fd < 0 { return 0; }
    let mut total = 0usize;
    while total < dst.len() {
        let n = sys_read(fd as u64, &mut dst[total..]);
        if n <= 0 { break; }
        total += n as usize;
    }
    sys_close(fd as u64);
    total
}

fn leer_todo(ruta_cstr: &[u8], dst: &mut [u8]) -> usize {
    let fd = sys_open(ruta_cstr, O_RDONLY);
    if fd < 0 { return 0; }
    let n = sys_read(fd as u64, dst);
    sys_close(fd as u64);
    if n > 0 { n as usize } else { 0 }
}

fn sonido_dsp(cmd: &[u8]) {
    let fd = sys_open(b"/dev/dsp\0", O_WRONLY);
    if fd >= 0 {
        sys_write(fd as u64, cmd);
        sys_close(fd as u64);
    }
}

fn red_comando(cmd: &[u8], dst: &mut [u8]) -> usize {
    let fd = sys_open(b"/dev/net\0", O_RDWR);
    if fd < 0 { return 0; }
    sys_write(fd as u64, cmd);
    let mut total = 0usize;
    while total < dst.len() {
        let n = sys_read(fd as u64, &mut dst[total..]);
        if n <= 0 { break; }
        total += n as usize;
    }
    sys_close(fd as u64);
    total
}

// --- Utilidades de texto ---

fn recortar(mut s: &[u8]) -> &[u8] {
    while let Some(&b) = s.first() {
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' { s = &s[1..]; } else { break; }
    }
    while let Some(&b) = s.last() {
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' { s = &s[..s.len() - 1]; } else { break; }
    }
    s
}

fn partir_palabra(s: &[u8]) -> (&[u8], &[u8]) {
    let s = recortar(s);
    for (i, &b) in s.iter().enumerate() {
        if b == b' ' || b == b'\t' {
            return (&s[..i], recortar(&s[i + 1..]));
        }
    }
    (s, b"")
}

fn dividir_byte(s: &[u8], sep: u8) -> Option<(&[u8], &[u8])> {
    for (i, &b) in s.iter().enumerate() {
        if b == sep { return Some((recortar(&s[..i]), recortar(&s[i + 1..]))); }
    }
    None
}

fn a_cstr<'a>(s: &[u8], dst: &'a mut [u8; 128]) -> Option<&'a [u8]> {
    if s.is_empty() || s.len() >= dst.len() { return None; }
    dst[..s.len()].copy_from_slice(s);
    dst[s.len()] = 0;
    Some(&dst[..=s.len()])
}

fn len_cstr(b: &[u8]) -> usize {
    let mut n = 0;
    while n < b.len() && b[n] != 0 { n += 1; }
    n
}

fn parse_u32(s: &[u8]) -> Option<u32> {
    let s = recortar(s);
    if s.is_empty() { return None; }
    let mut val = 0u32;
    for &b in s {
        if !b.is_ascii_digit() { return None; }
        val = val.saturating_mul(10).saturating_add((b - b'0') as u32);
    }
    Some(val)
}

fn parse_hex(s: &[u8]) -> Option<u32> {
    let mut s = recortar(s);
    if s.starts_with(b"#") { s = &s[1..]; }
    if s.starts_with(b"0x") { s = &s[2..]; }
    if s.is_empty() || s.len() > 6 { return None; }
    let mut val = 0u32;
    for &b in s {
        let d = match b {
            b'0'..=b'9' => (b - b'0') as u32,
            b'a'..=b'f' => (b - b'a' + 10) as u32,
            b'A'..=b'F' => (b - b'A' + 10) as u32,
            _ => return None,
        };
        val = (val << 4) | d;
    }
    Some(val)
}

fn fmt_2dig(n: u32, dst: &mut [u8], off: usize) {
    dst[off] = b'0' + ((n / 10) % 10) as u8;
    dst[off + 1] = b'0' + (n % 10) as u8;
}

// --- Reloj Real RTC parseado desde `/proc/time` ---

#[derive(Clone, Copy)]
struct RelojReal {
    anio: u32,
    mes: u32,
    dia: u32,
    hora: u32,
    minuto: u32,
    segundo: u32,
    dia_sem: u32,
    nombre_dia: [u8; 12],
    nombre_dia_len: usize,
    nombre_mes: [u8; 4],
}

impl RelojReal {
    fn defecto() -> Self {
        let mut r = RelojReal {
            anio: 2026, mes: 11, dia: 27, hora: 12, minuto: 0, segundo: 0, dia_sem: 5,
            nombre_dia: [0; 12], nombre_dia_len: 6, nombre_mes: *b"NOV ",
        };
        r.nombre_dia[..6].copy_from_slice(b"FRIDAY");
        r
    }

    fn refrescar(&mut self) {
        let mut buf = [0u8; 64];
        let n = leer_todo(b"/proc/time\0", &mut buf);
        if n < 20 { return; }
        // Formato: "2026-11-27 12:58:14 FRIDAY NOV 5"
        if let Some(v) = parse_u32(&buf[0..4]) { self.anio = v; }
        if let Some(v) = parse_u32(&buf[5..7]) { self.mes = v.clamp(1, 12); }
        if let Some(v) = parse_u32(&buf[8..10]) { self.dia = v.clamp(1, 31); }
        if let Some(v) = parse_u32(&buf[11..13]) { self.hora = v % 24; }
        if let Some(v) = parse_u32(&buf[14..16]) { self.minuto = v % 60; }
        if let Some(v) = parse_u32(&buf[17..19]) { self.segundo = v % 60; }
        let resto = recortar(&buf[20..n]);
        let (dia_w, resto2) = partir_palabra(resto);
        let dl = dia_w.len().min(12);
        self.nombre_dia[..dl].copy_from_slice(&dia_w[..dl]);
        self.nombre_dia_len = dl;
        let (mes_w, ds_w) = partir_palabra(resto2);
        if mes_w.len() >= 3 {
            self.nombre_mes[0..3].copy_from_slice(&mes_w[0..3]);
            self.nombre_mes[3] = 0;
        }
        if let Some(d) = parse_u32(ds_w) { self.dia_sem = d % 7; }
    }
}

// --- Tema configurable ---

#[derive(Clone, Copy)]
struct Tema {
    wallpaper: usize,
    opacidad: u32,   // 40..95
    blur: bool,
    oscuro: bool,
    brillo: u32,     // 45..100
    acento: u32,
}

impl Tema {
    const fn defecto() -> Self {
        Tema {
            wallpaper: 0, // 0 = Silk Waves monocromatico (como el video!)
            opacidad: 78,
            blur: true,
            oscuro: true,
            brillo: 100,
            acento: COL_AZUL_ACENTO,
        }
    }

    fn tinte(&self) -> u32 {
        if self.oscuro { 0x101318 } else { 0xE6EBF2 }
    }

    fn texto_principal(&self) -> u32 {
        if self.oscuro { COL_TEXTO } else { 0x111827 }
    }

    fn texto_secundario(&self) -> u32 {
        if self.oscuro { COL_TEXTO_SUAVE } else { 0x4B5563 }
    }

    fn borde_col(&self) -> u32 {
        if self.oscuro { 0xFFFFFF } else { 0x000000 }
    }

    fn alfa_ventana(&self) -> u32 {
        (self.opacidad * 255) / 100
    }
}

// --- Apps y Ventanas ---

const APP_TERMINAL: usize = 0;
const APP_MONITOR: usize = 1;
const APP_EDITOR: usize = 2;
const APP_PAINT: usize = 3;
const APP_BROWSER: usize = 4;
const APP_DOOM3D: usize = 5;
const NUM_APPS: usize = 6;

const TERM_FILAS: usize = 26;
const TERM_COLS: usize = 61;
const PAINT_W: usize = 520;
const PAINT_H: usize = 350;

#[derive(Clone, Copy)]
struct Ventana {
    abierta: bool,
    workspace: u8,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[derive(Clone, Copy)]
struct EntradaArchivo {
    nombre: [u8; 32],
    len: usize,
    es_dir: bool,
}

struct EstadoOS {
    en_login: bool,
    pass_buf: [u8; 32],
    pass_len: usize,
    login_error: bool,

    tema: Tema,
    cambiar_wallpaper_a: Option<usize>,
    workspace: u8,
    ventanas: [Ventana; NUM_APPS],
    orden_z: [usize; NUM_APPS],
    arrastrando: Option<(usize, i32, i32)>,
    salir_a_consola: bool,

    // Paneles flotantes (estilo del video!)
    mostrar_calendario: bool,
    mostrar_control: bool,
    mostrar_fondos: bool,

    // Raton
    mx: i32,
    my: i32,
    botones_prev: u32,
    botones: u32,

    // Reloj real + telemetria
    reloj: RelojReal,
    mon_ps: [u8; 320],
    mon_ps_len: usize,
    mon_disk: [u8; 256],
    mon_disk_len: usize,
    mon_uptime: [u8; 64],
    mon_uptime_len: usize,
    mon_mem: [u8; 32],
    mon_mem_len: usize,
    mon_uname: [u8; 96],
    mon_uname_len: usize,
    cpu_hist: [u8; 44],
    cpu_hist_idx: usize,

    // Terminal
    mostrar_fastfetch: bool,
    term_lineas: [[u8; TERM_COLS]; TERM_FILAS],
    term_colores: [u32; TERM_FILAS],
    term_len: [usize; TERM_FILAS],
    term_count: usize,
    term_input: [u8; 96],
    term_input_len: usize,
    cwd_cache: [u8; 48],
    cwd_len: usize,

    // Explorador + Editor real de codigo (guarda en disco NeoFS!)
    exp_dir: [u8; 96],
    exp_dir_len: usize,
    exp_entradas: [EntradaArchivo; 16],
    exp_count: usize,
    exp_preview: [u8; 4096],
    exp_preview_len: usize,
    exp_cursor: usize,
    exp_scroll: usize,
    exp_modificado: bool,
    exp_msg_guardado: u8,
    exp_nuevo_seq: u32,
    exp_sel_nombre: [u8; 64],
    exp_sel_len: usize,

    // Portapapeles compartido del sistema (Ctrl+C / Ctrl+V)
    portapapeles: [u8; 256],
    portapapeles_len: usize,

    // Paint
    paint_color: u32,
    paint_buf: *mut u32,

    // Navegador Web real (NeoWeb sobre e1000) + Motor 3D FPS (NeoDoom)
    mon_net: [u8; 256],
    mon_net_len: usize,
    navegador: web::Navegador,
    doom: doom3d::Motor3D,
}

impl EstadoOS {
    fn inicializar(&mut self, paint_buf: *mut u32) {
        self.en_login = true;
        self.pass_len = 0;
        self.login_error = false;

        self.tema = Tema::defecto();
        self.cambiar_wallpaper_a = None;
        self.workspace = 0;

        // Workspace 1: limpio (reloj central minimalista + dock, como 00:00 del video!)
        // Workspace 2: Terminal (izq) + btop Monitor (der) en mosaico suave (como 00:14-00:16!)
        // Workspace 3: Explorador + Visor de Codigo (como 00:20!)
        self.ventanas[APP_TERMINAL] = Ventana { abierta: true, workspace: 1, x: 24, y: 50, w: 608, h: 586 };
        self.ventanas[APP_MONITOR] = Ventana { abierta: true, workspace: 1, x: 648, y: 50, w: 608, h: 586 };
        self.ventanas[APP_EDITOR] = Ventana { abierta: true, workspace: 2, x: 64, y: 56, w: 1152, h: 572 };
        self.ventanas[APP_PAINT] = Ventana { abierta: false, workspace: 0, x: 350, y: 110, w: 564, h: 454 };
        self.ventanas[APP_BROWSER] = Ventana { abierta: false, workspace: 0, x: 140, y: 54, w: 1000, h: 574 };
        self.ventanas[APP_DOOM3D] = Ventana { abierta: false, workspace: 0, x: 352, y: 104, w: 576, h: 432 };

        self.orden_z = [APP_DOOM3D, APP_BROWSER, APP_PAINT, APP_EDITOR, APP_MONITOR, APP_TERMINAL];
        self.arrastrando = None;
        self.salir_a_consola = false;
        self.mostrar_calendario = false;
        self.mostrar_control = false;
        self.mostrar_fondos = false;

        self.mx = 640;
        self.my = 360;
        self.botones = 0;
        self.botones_prev = 0;

        self.reloj = RelojReal::defecto();
        self.reloj.refrescar();

        // Curva inicial suave para la grafica estilo btop/cava
        for i in 0..self.cpu_hist.len() {
            self.cpu_hist[i] = (14 + ((i * 7 + 5) % 24)) as u8;
        }
        self.cpu_hist_idx = 0;

        self.mostrar_fastfetch = true;
        self.term_count = 0;
        self.term_input_len = 0;
        self.actualizar_cwd();

        self.exp_dir[..15].copy_from_slice(b"/home/guillermo");
        self.exp_dir_len = 15;
        self.exp_cursor = 0;
        self.exp_scroll = 0;
        self.exp_modificado = false;
        self.exp_msg_guardado = 0;
        self.exp_nuevo_seq = 1;
        self.portapapeles_len = 0;
        self.cargar_tema_disco();
        self.exp_recargar();
        self.exp_abrir_archivo(b"/home/guillermo/ejemplo.py");

        self.monitor_refrescar();

        self.paint_color = COL_CIAN;
        self.paint_buf = paint_buf;
        self.paint_limpiar();

        self.mon_net_len = leer_todo(b"/proc/net\0", &mut self.mon_net);
        self.navegador.inicializar();
        self.navegador.cargar_home(&self.mon_net[..self.mon_net_len]);
        self.doom = doom3d::Motor3D::nuevo();
    }

    fn cargar_tema_disco(&mut self) {
        let mut buf = [0u8; 512];
        let n = leer_todo(b"/etc/neo_ws/theme.conf\0", &mut buf);
        if n == 0 { return; }
        let mut ini = 0;
        for i in 0..=n {
            if i == n || buf[i] == b'\n' {
                let linea = recortar(&buf[ini..i]);
                if !linea.is_empty() && linea[0] != b'#' {
                    if let Some((k, v)) = dividir_byte(linea, b'=') {
                        match k {
                            b"wallpaper" => if let Some(x) = parse_u32(v) { self.tema.wallpaper = (x as usize) % 4; },
                            b"opacity" => if let Some(x) = parse_u32(v) { self.tema.opacidad = x.clamp(40, 95); },
                            b"blur" => if let Some(x) = parse_u32(v) { self.tema.blur = x != 0; },
                            b"dark" => if let Some(x) = parse_u32(v) { self.tema.oscuro = x != 0; },
                            b"brightness" => if let Some(x) = parse_u32(v) { self.tema.brillo = x.clamp(45, 100); },
                            b"accent" => if let Some(c) = parse_hex(v) { self.tema.acento = c; },
                            _ => {}
                        }
                    }
                }
                ini = i + 1;
            }
        }
    }

    fn guardar_tema_disco(&self) {
        let fd = sys_open(b"/etc/neo_ws/theme.conf\0", O_WRONLY | O_CREAT | O_TRUNC);
        if fd < 0 { return; }
        let mut out = [0u8; 192];
        let mut p = 0usize;
        let mut emp = |s: &[u8]| {
            let n = s.len().min(out.len() - p);
            out[p..p + n].copy_from_slice(&s[..n]);
            p += n;
        };
        emp(b"# neo_ws tema persistente en NeoFS\nwallpaper=");
        let w = [b'0' + (self.tema.wallpaper % 4) as u8];
        emp(&w);
        emp(b"\nopacity=");
        let mut d2 = [0u8; 2];
        fmt_2dig(self.tema.opacidad, &mut d2, 0);
        emp(&d2);
        emp(b"\nblur=");
        emp(if self.tema.blur { b"1" } else { b"0" });
        emp(b"\ndark=");
        emp(if self.tema.oscuro { b"1" } else { b"0" });
        emp(b"\naccent=#");
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut h = [0u8; 6];
        for i in 0..6 {
            h[5 - i] = HEX[((self.tema.acento >> (i * 4)) & 0xF) as usize];
        }
        emp(&h);
        emp(b"\n");
        sys_write(fd as u64, &out[..p]);
        sys_close(fd as u64);
    }

    fn actualizar_cwd(&mut self) {
        let mut buf = [0u8; 64];
        if sys_getcwd(&mut buf) > 0 {
            let n = len_cstr(&buf).min(self.cwd_cache.len());
            self.cwd_cache[..n].copy_from_slice(&buf[..n]);
            self.cwd_len = n;
        } else {
            self.cwd_cache[0] = b'/';
            self.cwd_len = 1;
        }
    }

    fn traer_al_frente(&mut self, app: usize) {
        if let Some(pos) = self.orden_z.iter().position(|&a| a == app) {
            for i in pos..NUM_APPS - 1 {
                self.orden_z[i] = self.orden_z[i + 1];
            }
            self.orden_z[NUM_APPS - 1] = app;
        }
    }

    fn abrir_o_enfocar(&mut self, app: usize) {
        self.mostrar_calendario = false;
        self.mostrar_control = false;
        self.mostrar_fondos = false;
        if self.ventanas[app].abierta && self.ventanas[app].workspace != self.workspace {
            self.workspace = self.ventanas[app].workspace;
        } else {
            self.ventanas[app].workspace = self.workspace;
            self.ventanas[app].abierta = true;
        }
        self.traer_al_frente(app);
        if app == APP_MONITOR { self.monitor_refrescar(); }
        if app == APP_EDITOR { self.exp_recargar(); }
    }

    fn foco_actual(&self) -> Option<usize> {
        for &app in self.orden_z.iter().rev() {
            let v = self.ventanas[app];
            if v.abierta && v.workspace == self.workspace {
                return Some(app);
            }
        }
        None
    }

    fn hay_ventana_en_workspace(&self) -> bool {
        self.ventanas.iter().any(|v| v.abierta && v.workspace == self.workspace)
    }

    fn verificar_login(&mut self) {
        let pwd = &self.pass_buf[..self.pass_len];
        // Acepta contrasena "neo" (de /etc/passwd) o Enter vacio para entrar rapido.
        if pwd.is_empty() || pwd == b"neo" || pwd == b"1234" {
            self.en_login = false;
            self.pass_len = 0;
            self.login_error = false;
            sonido_dsp(b"chime");
        } else {
            self.login_error = true;
            self.pass_len = 0;
        }
    }

    // --- Terminal ---

    fn max_filas_term(&self) -> usize {
        if self.mostrar_fastfetch { 14 } else { 26 }
    }

    fn term_agregar(&mut self, texto: &[u8], col: u32) {
        if texto.is_empty() { self.term_empujar(b"", col); return; }
        let mut off = 0;
        while off < texto.len() {
            let fin = (off + TERM_COLS).min(texto.len());
            self.term_empujar(&texto[off..fin], col);
            off = fin;
        }
    }

    fn term_empujar(&mut self, slice: &[u8], col: u32) {
        let lim = self.max_filas_term();
        if self.term_count >= lim {
            let exceso = self.term_count + 1 - lim;
            for i in exceso..self.term_count {
                self.term_lineas[i - exceso] = self.term_lineas[i];
                self.term_colores[i - exceso] = self.term_colores[i];
                self.term_len[i - exceso] = self.term_len[i];
            }
            self.term_count = lim - 1;
        }
        let idx = self.term_count;
        let n = slice.len().min(TERM_COLS);
        self.term_lineas[idx][..n].copy_from_slice(&slice[..n]);
        self.term_len[idx] = n;
        self.term_colores[idx] = col;
        self.term_count += 1;
    }

    fn term_imprimir_salida(&mut self, datos: &[u8], col_def: u32) {
        let mut linea = [0u8; TERM_COLS];
        let mut l = 0usize;
        let mut col = col_def;
        let mut ansi: u8 = 0;
        let mut csi = [0u8; 8];
        let mut csi_l = 0usize;

        for &b in datos {
            match ansi {
                1 => { if b == b'[' { ansi = 2; csi_l = 0; } else { ansi = 0; } continue; }
                2 => {
                    if b.is_ascii_digit() || b == b';' {
                        if csi_l < csi.len() { csi[csi_l] = b; csi_l += 1; }
                    } else {
                        if b == b'm' {
                            col = match &csi[..csi_l] {
                                b"92" | b"32" => COL_VERDE,
                                b"96" | b"36" => COL_CIAN,
                                b"93" | b"33" => COL_AMARILLO,
                                b"91" | b"31" => COL_ROJO,
                                _ => col_def,
                            };
                        }
                        ansi = 0;
                    }
                    continue;
                }
                _ => {}
            }
            if b == 0x1b { ansi = 1; continue; }
            if b == b'\n' {
                self.term_empujar(&linea[..l], col);
                l = 0;
            } else if b >= 32 && b <= 126 {
                if l >= TERM_COLS {
                    self.term_empujar(&linea[..l], col);
                    l = 0;
                }
                linea[l] = b;
                l += 1;
            }
        }
        if l > 0 { self.term_empujar(&linea[..l], col); }
    }

    fn term_ejecutar(&mut self) {
        let mut copia = [0u8; 96];
        let len = self.term_input_len;
        copia[..len].copy_from_slice(&self.term_input[..len]);
        self.term_input_len = 0;

        let linea = recortar(&copia[..len]);
        if linea.is_empty() { return; }

        let mut eco = [0u8; 108];
        eco[0] = b'$';
        eco[1] = b' ';
        eco[2..2 + linea.len()].copy_from_slice(linea);
        self.term_agregar(&eco[..2 + linea.len()], COL_VERDE);

        let (cmd, args) = partir_palabra(linea);
        if dividir_byte(linea, b'|').is_none() && dividir_byte(linea, b'>').is_none() {
            match cmd {
                b"clear" => { self.mostrar_fastfetch = false; self.term_count = 0; return; }
                b"fastfetch" | b"neofetch" => {
                    self.mostrar_fastfetch = true;
                    self.term_count = 0;
                    self.monitor_refrescar();
                    return;
                }
                b"help" => {
                    self.mostrar_fastfetch = false;
                    self.term_agregar(b"Sistema: fastfetch, clear, ls, cat, cd, pwd, write, ps, disk, uname", COL_TEXTO);
                    self.term_agregar(b"Red/Audio: net, ping <host>, dns <host>, curl <url>, beep [hz]", COL_CIAN);
                    self.term_agregar(b"Temas: rice opacity <40-95> | blur <0|1> | wall <0-3> | lock", COL_AMARILLO);
                    self.term_agregar(b"Linux ELF: hola, hola_std (musl/std), forktest, upper | exit", COL_VERDE);
                    return;
                }
                b"lock" => { self.en_login = true; self.pass_len = 0; return; }
                b"rice" => {
                    let (k, v) = partir_palabra(args);
                    match k {
                        b"opacity" => if let Some(x) = parse_u32(v) { self.tema.opacidad = x.clamp(40, 96); },
                        b"blur" => if let Some(x) = parse_u32(v) { self.tema.blur = x != 0; },
                        b"wall" => if let Some(x) = parse_u32(v) { self.cambiar_wallpaper_a = Some((x as usize) % 4); },
                        b"accent" => if let Some(c) = parse_hex(v) { self.tema.acento = c; },
                        b"dark" => if let Some(x) = parse_u32(v) { self.tema.oscuro = x != 0; },
                        _ => self.term_agregar(b"Uso: rice opacity|blur|wall|accent|dark <valor>", COL_AMARILLO),
                    }
                    return;
                }
                b"cd" => {
                    let ruta = if args.is_empty() { b"/" } else { args };
                    let mut cbuf = [0u8; 128];
                    if let Some(cstr) = a_cstr(ruta, &mut cbuf) {
                        if sys_chdir(cstr) == 0 { self.actualizar_cwd(); }
                        else { self.term_agregar(b"cd: directorio invalido", COL_ROJO); }
                    }
                    return;
                }
                b"pwd" => {
                    let mut c = [0u8; 48];
                    let n = self.cwd_len;
                    c[..n].copy_from_slice(&self.cwd_cache[..n]);
                    self.term_agregar(&c[..n], COL_TEXTO);
                    return;
                }
                b"ls" => { self.term_cmd_ls(args); return; }
                b"cat" => { self.term_cmd_cat(args); return; }
                b"ps" => { self.term_cmd_cat(b"/proc/ps"); return; }
                b"disk" => { self.term_cmd_cat(b"/proc/disk"); return; }
                b"net" => { self.term_cmd_cat(b"/proc/net"); return; }
                b"ping" => {
                    let host = if args.is_empty() { b"10.0.2.2" } else { args };
                    let mut q = [0u8; 96];
                    q[..5].copy_from_slice(b"ping ");
                    let n = host.len().min(88);
                    q[5..5 + n].copy_from_slice(&host[..n]);
                    let mut resp = [0u8; 512];
                    let rl = red_comando(&q[..5 + n], &mut resp);
                    self.term_imprimir_salida(&resp[..rl], COL_CIAN);
                    self.monitor_refrescar();
                    return;
                }
                b"dns" => {
                    let host = if args.is_empty() { b"example.com" } else { args };
                    let mut q = [0u8; 96];
                    q[..4].copy_from_slice(b"dns ");
                    let n = host.len().min(88);
                    q[4..4 + n].copy_from_slice(&host[..n]);
                    let mut resp = [0u8; 256];
                    let rl = red_comando(&q[..4 + n], &mut resp);
                    self.term_imprimir_salida(&resp[..rl], COL_VERDE);
                    return;
                }
                b"curl" | b"fetch" => {
                    let url = if args.is_empty() { b"http://example.com/" } else { args };
                    let mut q = [0u8; 128];
                    q[..4].copy_from_slice(b"get ");
                    let n = url.len().min(120);
                    q[4..4 + n].copy_from_slice(&url[..n]);
                    let mut resp = [0u8; 768];
                    let rl = red_comando(&q[..4 + n], &mut resp);
                    self.term_imprimir_salida(&resp[..rl.min(360)], COL_TEXTO);
                    self.monitor_refrescar();
                    return;
                }
                b"beep" => {
                    if args.is_empty() {
                        sonido_dsp(b"chime");
                    } else {
                        let mut q = [0u8; 32];
                        q[..5].copy_from_slice(b"tone ");
                        let n = args.len().min(20);
                        q[5..5 + n].copy_from_slice(&args[..n]);
                        sonido_dsp(&q[..5 + n]);
                    }
                    self.term_agregar(b"Audio AC97 reproducido (/dev/dsp).", COL_VERDE);
                    return;
                }
                b"uname" => {
                    let mut u = [0u8; 96];
                    let n = self.mon_uname_len;
                    u[..n].copy_from_slice(&self.mon_uname[..n]);
                    self.term_agregar(&u[..n], COL_TEXTO);
                    return;
                }
                b"write" => {
                    let (ruta, txt) = partir_palabra(args);
                    let mut cbuf = [0u8; 128];
                    if let Some(cstr) = a_cstr(ruta, &mut cbuf) {
                        let fd = sys_open(cstr, O_WRONLY | O_CREAT | O_TRUNC);
                        if fd >= 0 {
                            sys_write(fd as u64, txt);
                            sys_write(fd as u64, b"\n");
                            sys_close(fd as u64);
                            self.term_agregar(b"Archivo guardado.", COL_VERDE);
                            self.exp_recargar();
                            return;
                        }
                    }
                    self.term_agregar(b"Uso: write <ruta> <texto>", COL_AMARILLO);
                    return;
                }
                b"exit" => { self.salir_a_consola = true; return; }
                _ => {}
            }
        }

        self.term_ejecutar_subproceso(linea);
        self.monitor_refrescar();
    }

    fn term_cmd_ls(&mut self, ruta: &[u8]) {
        let ruta = if ruta.is_empty() { b"." } else { ruta };
        let mut cbuf = [0u8; 128];
        let Some(cstr) = a_cstr(ruta, &mut cbuf) else { return; };
        let fd = sys_open(cstr, O_RDONLY);
        if fd < 0 { self.term_agregar(b"ls: ruta no encontrada", COL_ROJO); return; }
        let mut buf = [0u8; 512];
        let mut out = [0u8; 256];
        let mut o = 0usize;
        loop {
            let n = sys_getdents64(fd as u64, &mut buf);
            if n <= 0 { break; }
            let mut pos = 0usize;
            while pos + 19 < n as usize {
                let reclen = u16::from_le_bytes([buf[pos + 16], buf[pos + 17]]) as usize;
                if reclen < 19 || pos + reclen > n as usize { break; }
                let dtype = buf[pos + 18];
                let nombre = &buf[pos + 19..pos + reclen];
                let nl = len_cstr(nombre);
                if o + nl + 3 < out.len() {
                    out[o..o + nl].copy_from_slice(&nombre[..nl]);
                    o += nl;
                    if dtype == DT_DIR { out[o] = b'/'; o += 1; }
                    out[o] = b' '; o += 1;
                    out[o] = b' '; o += 1;
                }
                pos += reclen;
            }
        }
        sys_close(fd as u64);
        self.term_agregar(&out[..o], COL_CIAN);
    }

    fn term_cmd_cat(&mut self, ruta: &[u8]) {
        let mut cbuf = [0u8; 128];
        let Some(cstr) = a_cstr(ruta, &mut cbuf) else { return; };
        let fd = sys_open(cstr, O_RDONLY);
        if fd < 0 { self.term_agregar(b"cat: no se pudo abrir", COL_ROJO); return; }
        let mut buf = [0u8; 512];
        let n = sys_read(fd as u64, &mut buf);
        sys_close(fd as u64);
        if n > 0 { self.term_imprimir_salida(&buf[..n as usize], COL_TEXTO); }
    }

    fn term_ejecutar_subproceso(&mut self, linea: &[u8]) {
        let mut cap = [0i32; 2];
        if sys_pipe(&mut cap) < 0 { return; }

        if let Some((izq, der)) = dividir_byte(linea, b'|') {
            let mut mid = [0i32; 2];
            if sys_pipe(&mut mid) < 0 {
                sys_close(cap[0] as u64); sys_close(cap[1] as u64); return;
            }
            let p1 = sys_fork();
            if p1 == 0 {
                sys_dup2(mid[1] as u64, 1); sys_dup2(mid[1] as u64, 2);
                sys_close(mid[0] as u64); sys_close(mid[1] as u64);
                sys_close(cap[0] as u64); sys_close(cap[1] as u64);
                ejecutar_elf_o_morir(izq);
            }
            let p2 = sys_fork();
            if p2 == 0 {
                sys_dup2(mid[0] as u64, 0);
                sys_dup2(cap[1] as u64, 1); sys_dup2(cap[1] as u64, 2);
                sys_close(mid[0] as u64); sys_close(mid[1] as u64);
                sys_close(cap[0] as u64); sys_close(cap[1] as u64);
                ejecutar_elf_o_morir(der);
            }
            sys_close(mid[0] as u64); sys_close(mid[1] as u64); sys_close(cap[1] as u64);
            let mut st = 0i32;
            if p1 > 0 { sys_wait4(p1 as i64, &mut st); }
            if p2 > 0 { sys_wait4(p2 as i64, &mut st); }
        } else {
            let hijo = sys_fork();
            if hijo == 0 {
                sys_dup2(cap[1] as u64, 1); sys_dup2(cap[1] as u64, 2);
                sys_close(cap[0] as u64); sys_close(cap[1] as u64);
                ejecutar_elf_o_morir(linea);
            }
            sys_close(cap[1] as u64);
            let mut st = 0i32;
            if hijo > 0 { sys_wait4(hijo as i64, &mut st); }
        }

        let mut salida = [0u8; 1024];
        let mut total = 0usize;
        loop {
            if total >= salida.len() { break; }
            let n = sys_read(cap[0] as u64, &mut salida[total..]);
            if n <= 0 { break; }
            total += n as usize;
        }
        sys_close(cap[0] as u64);
        if total > 0 { self.term_imprimir_salida(&salida[..total], COL_TEXTO); }
    }

    // --- Explorador, Monitor y Paint ---

    fn exp_recargar(&mut self) {
        let mut cbuf = [0u8; 128];
        let Some(cstr) = a_cstr(&self.exp_dir[..self.exp_dir_len], &mut cbuf) else { return; };
        let fd = sys_open(cstr, O_RDONLY);
        if fd < 0 { return; }
        self.exp_count = 0;
        let mut buf = [0u8; 512];
        loop {
            let n = sys_getdents64(fd as u64, &mut buf);
            if n <= 0 { break; }
            let mut pos = 0usize;
            while pos + 19 < n as usize && self.exp_count < self.exp_entradas.len() {
                let reclen = u16::from_le_bytes([buf[pos + 16], buf[pos + 17]]) as usize;
                if reclen < 19 || pos + reclen > n as usize { break; }
                let dtype = buf[pos + 18];
                let ns = &buf[pos + 19..pos + reclen];
                let nl = len_cstr(ns).min(31);
                let mut e = EntradaArchivo { nombre: [0; 32], len: nl, es_dir: dtype == DT_DIR };
                e.nombre[..nl].copy_from_slice(&ns[..nl]);
                self.exp_entradas[self.exp_count] = e;
                self.exp_count += 1;
                pos += reclen;
            }
        }
        sys_close(fd as u64);
    }

    fn exp_abrir_archivo(&mut self, ruta: &[u8]) {
        let mut cbuf = [0u8; 128];
        let Some(cstr) = a_cstr(ruta, &mut cbuf) else { return; };
        let fd = sys_open(cstr, O_RDONLY);
        if fd < 0 { return; }
        let n = sys_read(fd as u64, &mut self.exp_preview);
        sys_close(fd as u64);
        self.exp_preview_len = if n > 0 { n as usize } else { 0 };
        let nl = ruta.len().min(self.exp_sel_nombre.len());
        self.exp_sel_nombre[..nl].copy_from_slice(&ruta[..nl]);
        self.exp_sel_len = nl;
        self.exp_cursor = self.exp_preview_len;
        self.exp_scroll = 0;
        self.exp_modificado = false;
    }

    fn exp_guardar(&mut self) {
        if self.exp_sel_len == 0 { return; }
        let mut cbuf = [0u8; 128];
        let ruta = &self.exp_sel_nombre[..self.exp_sel_len];
        let Some(cstr) = a_cstr(ruta, &mut cbuf) else { return; };
        let fd = sys_open(cstr, O_WRONLY | O_CREAT | O_TRUNC);
        if fd >= 0 {
            sys_write(fd as u64, &self.exp_preview[..self.exp_preview_len]);
            sys_close(fd as u64);
            self.exp_modificado = false;
            self.exp_msg_guardado = 3;
            self.monitor_refrescar();
        }
    }

    fn exp_nuevo(&mut self) {
        let mut nombre = [0u8; 64];
        let pref = b"/home/guillermo/nota_";
        let mut p = pref.len();
        nombre[..p].copy_from_slice(pref);
        let n = self.exp_nuevo_seq;
        self.exp_nuevo_seq += 1;
        if n >= 10 {
            nombre[p] = b'0' + ((n / 10) % 10) as u8;
            p += 1;
        }
        nombre[p] = b'0' + (n % 10) as u8;
        p += 1;
        let ext = b".txt";
        nombre[p..p + ext.len()].copy_from_slice(ext);
        p += ext.len();

        let mut cbuf = [0u8; 128];
        if let Some(cstr) = a_cstr(&nombre[..p], &mut cbuf) {
            let fd = sys_open(cstr, O_WRONLY | O_CREAT | O_TRUNC);
            if fd >= 0 {
                let init = b"# Nueva nota en neo_ws (guardada en NeoFS)\n";
                sys_write(fd as u64, init);
                sys_close(fd as u64);
            }
        }
        self.exp_dir[..15].copy_from_slice(b"/home/guillermo");
        self.exp_dir_len = 15;
        self.exp_recargar();
        self.exp_abrir_archivo(&nombre[..p]);
        self.exp_msg_guardado = 2;
    }

    fn exp_insertar_byte(&mut self, b: u8) {
        if self.exp_preview_len + 1 >= self.exp_preview.len() { return; }
        let pos = self.exp_cursor.min(self.exp_preview_len);
        for i in (pos..self.exp_preview_len).rev() {
            self.exp_preview[i + 1] = self.exp_preview[i];
        }
        self.exp_preview[pos] = b;
        self.exp_preview_len += 1;
        self.exp_cursor = pos + 1;
        self.exp_modificado = true;
        self.exp_asegurar_scroll();
    }

    fn exp_borrar_atras(&mut self) {
        if self.exp_cursor == 0 || self.exp_preview_len == 0 { return; }
        let pos = self.exp_cursor.min(self.exp_preview_len);
        for i in pos..self.exp_preview_len {
            self.exp_preview[i - 1] = self.exp_preview[i];
        }
        self.exp_preview_len -= 1;
        self.exp_cursor = pos - 1;
        self.exp_modificado = true;
        self.exp_asegurar_scroll();
    }

    fn exp_borrar_supr(&mut self) {
        let pos = self.exp_cursor.min(self.exp_preview_len);
        if pos >= self.exp_preview_len { return; }
        for i in pos + 1..self.exp_preview_len {
            self.exp_preview[i - 1] = self.exp_preview[i];
        }
        self.exp_preview_len -= 1;
        self.exp_modificado = true;
    }

    fn exp_linea_col_cursor(&self) -> (usize, usize) {
        let pos = self.exp_cursor.min(self.exp_preview_len);
        let mut linea = 0usize;
        let mut col = 0usize;
        for i in 0..pos {
            if self.exp_preview[i] == b'\n' {
                linea += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        (linea, col)
    }

    fn exp_ir_a_linea_col(&mut self, obj_linea: usize, obj_col: usize) {
        let mut linea = 0usize;
        let mut col = 0usize;
        for i in 0..self.exp_preview_len {
            if linea == obj_linea && (col == obj_col || self.exp_preview[i] == b'\n') {
                self.exp_cursor = i;
                self.exp_asegurar_scroll();
                return;
            }
            if self.exp_preview[i] == b'\n' {
                linea += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        self.exp_cursor = self.exp_preview_len;
        self.exp_asegurar_scroll();
    }

    fn exp_asegurar_scroll(&mut self) {
        let (linea, _) = self.exp_linea_col_cursor();
        let visibles = 23usize;
        if linea < self.exp_scroll {
            self.exp_scroll = linea;
        } else if linea >= self.exp_scroll + visibles {
            self.exp_scroll = linea + 1 - visibles;
        }
    }

    fn exp_copiar_linea(&mut self) {
        let pos = self.exp_cursor.min(self.exp_preview_len);
        let mut ini = pos;
        while ini > 0 && self.exp_preview[ini - 1] != b'\n' { ini -= 1; }
        let mut fin = pos;
        while fin < self.exp_preview_len && self.exp_preview[fin] != b'\n' { fin += 1; }
        let n = (fin - ini).min(self.portapapeles.len());
        self.portapapeles[..n].copy_from_slice(&self.exp_preview[ini..ini + n]);
        self.portapapeles_len = n;
    }

    fn exp_pegar(&mut self) {
        let mut tmp = [0u8; 256];
        let n = self.portapapeles_len;
        tmp[..n].copy_from_slice(&self.portapapeles[..n]);
        for i in 0..n {
            self.exp_insertar_byte(tmp[i]);
        }
    }

    fn exp_subir(&mut self) {
        if self.exp_dir_len <= 1 { return; }
        let mut i = self.exp_dir_len - 1;
        while i > 0 && self.exp_dir[i] != b'/' { i -= 1; }
        self.exp_dir_len = if i == 0 { 1 } else { i };
        self.exp_recargar();
    }

    fn exp_clic(&mut self, idx: usize) {
        if idx >= self.exp_count { return; }
        let e = self.exp_entradas[idx];
        let mut ruta = [0u8; 96];
        let mut rl;
        if self.exp_dir_len == 1 {
            ruta[0] = b'/';
            rl = 1;
        } else {
            ruta[..self.exp_dir_len].copy_from_slice(&self.exp_dir[..self.exp_dir_len]);
            rl = self.exp_dir_len;
            if rl < ruta.len() { ruta[rl] = b'/'; rl += 1; }
        }
        let c = e.len.min(ruta.len() - rl);
        ruta[rl..rl + c].copy_from_slice(&e.nombre[..c]);
        rl += c;

        if e.es_dir {
            self.exp_dir[..rl].copy_from_slice(&ruta[..rl]);
            self.exp_dir_len = rl;
            self.exp_recargar();
        } else {
            self.exp_abrir_archivo(&ruta[..rl]);
            if self.exp_preview_len >= 4 && &self.exp_preview[0..4] == b"\x7fELF" {
                let msg = b"# Ejecutable ELF64 Linux x86_64\n# Ejecutalo desde la Terminal de neo_ws!\n";
                self.exp_preview[..msg.len()].copy_from_slice(msg);
                self.exp_preview_len = msg.len();
            }
        }
    }

    fn monitor_refrescar(&mut self) {
        self.reloj.refrescar();
        self.mon_ps_len = leer_todo(b"/proc/ps\0", &mut self.mon_ps);
        self.mon_disk_len = leer_todo(b"/proc/disk\0", &mut self.mon_disk);
        self.mon_net_len = leer_todo(b"/proc/net\0", &mut self.mon_net);
        self.mon_uptime_len = leer_todo(b"/proc/uptime\0", &mut self.mon_uptime);
        self.mon_mem_len = leer_todo(b"/proc/meminfo\0", &mut self.mon_mem);
        while self.mon_mem_len > 0 && self.mon_mem[self.mon_mem_len - 1] <= 32 {
            self.mon_mem_len -= 1;
        }
        while self.mon_uptime_len > 0 && self.mon_uptime[self.mon_uptime_len - 1] <= 32 {
            self.mon_uptime_len -= 1;
        }
        let mut u = UtsName {
            sysname: [0; 65], nodename: [0; 65], release: [0; 65],
            version: [0; 65], machine: [0; 65], domainname: [0; 65],
        };
        if sys_uname(&mut u) == 0 {
            let mut o = 0usize;
            for f in [&u.sysname[..], &u.nodename[..], &u.release[..], &u.machine[..]] {
                let n = len_cstr(f);
                if o + n + 1 < self.mon_uname.len() {
                    self.mon_uname[o..o + n].copy_from_slice(&f[..n]);
                    o += n;
                    self.mon_uname[o] = b' ';
                    o += 1;
                }
            }
            self.mon_uname_len = o;
        }
        // Actualizamos muestra para grafica estilo btop
        let s = sys_uptime_sec() as usize;
        let muestra = (16 + ((s * 17 + self.mon_ps_len) % 34)) as u8;
        self.cpu_hist[self.cpu_hist_idx] = muestra;
        self.cpu_hist_idx = (self.cpu_hist_idx + 1) % self.cpu_hist.len();
    }

    fn paint_limpiar(&mut self) {
        if self.paint_buf.is_null() { return; }
        unsafe {
            core::slice::from_raw_parts_mut(self.paint_buf, PAINT_W * PAINT_H).fill(0x0D1118);
        }
    }

    fn paint_punto(&mut self, lx: i32, ly: i32) {
        if self.paint_buf.is_null() { return; }
        let col = self.paint_color;
        for dy in -2..=2 {
            for dx in -2..=2 {
                if dx * dx + dy * dy <= 5 {
                    let px = lx + dx;
                    let py = ly + dy;
                    if px >= 0 && py >= 0 && (px as usize) < PAINT_W && (py as usize) < PAINT_H {
                        unsafe { *self.paint_buf.add((py as usize) * PAINT_W + (px as usize)) = col; }
                    }
                }
            }
        }
    }

    fn paint_linea(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        let pasos = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for i in 0..=pasos {
            self.paint_punto(x0 + (x1 - x0) * i / pasos, y0 + (y1 - y0) * i / pasos);
        }
    }

    // --- Eventos ---

    fn hover_firma(&self) -> u32 {
        if self.en_login {
            let cx = (W as i32) / 2;
            let h = self.mx >= cx - 72 && self.mx < cx + 72 && self.my >= 476 && self.my < 510;
            return if h { 1 } else { 0 };
        }
        let mut sig = 0u32;
        if self.my < 38 && self.mx >= 112 && self.mx < 186 {
            sig |= 1;
        }
        let dock_w = 424;
        let dx = (W as i32 - dock_w) / 2;
        let dy = H as i32 - 58 - 10;
        for i in 0..8 {
            let ix = dx + 16 + (i as i32) * 50;
            let iy = dy + 7;
            if self.mx >= ix && self.mx < ix + 38 && self.my >= iy && self.my < iy + 38 {
                sig |= 1 << (i + 2);
            }
        }
        sig
    }

    /// Devuelve `true` si la escena necesita redibujarse, `false` si solo se movio el cursor.
    fn procesar_evento(&mut self, ev: EventoEntrada) -> bool {
        match ev.tipo {
            TIPO_RATON => {
                let prev_x = self.mx;
                let prev_y = self.my;
                let hover_antes = self.hover_firma();
                self.mx = ev.x.clamp(0, W as i32 - 1);
                self.my = ev.y.clamp(0, H as i32 - 1);
                self.botones_prev = self.botones;
                self.botones = ev.botones;

                let izq_ahora = (self.botones & 1) != 0;
                let izq_antes = (self.botones_prev & 1) != 0;
                let mut cambio_escena = self.hover_firma() != hover_antes;

                // Rueda del raton (IntelliMouse scroll)
                let rueda = ev.codigo as i16;
                if rueda != 0 && !self.en_login {
                    if self.foco_actual() == Some(APP_EDITOR) {
                        if rueda > 0 {
                            self.exp_scroll = self.exp_scroll.saturating_add(2);
                        } else {
                            self.exp_scroll = self.exp_scroll.saturating_sub(2);
                        }
                        cambio_escena = true;
                    } else if self.foco_actual() == Some(APP_BROWSER) {
                        if rueda > 0 {
                            self.navegador.scroll = self.navegador.scroll.saturating_add(2);
                        } else {
                            self.navegador.scroll = self.navegador.scroll.saturating_sub(2);
                        }
                        cambio_escena = true;
                    }
                }

                if izq_ahora && !izq_antes {
                    if self.en_login {
                        self.on_clic_login(self.mx, self.my);
                    } else {
                        self.on_clic_escritorio(self.mx, self.my);
                    }
                    cambio_escena = true;
                } else if !izq_ahora && izq_antes {
                    if self.arrastrando.is_some() {
                        cambio_escena = true;
                    }
                    self.arrastrando = None;
                } else if izq_ahora && !self.en_login {
                    if let Some((app, ox, oy)) = self.arrastrando {
                        let nx = (self.mx - ox).clamp(8, W as i32 - 120);
                        let ny = (self.my - oy).clamp(44, H as i32 - 80);
                        self.ventanas[app].x = nx;
                        self.ventanas[app].y = ny;
                        cambio_escena = true;
                    } else if self.foco_actual() == Some(APP_PAINT) {
                        let v = self.ventanas[APP_PAINT];
                        let ox = v.x + 22;
                        let oy = v.y + 82;
                        let lx = self.mx - ox;
                        let ly = self.my - oy;
                        let px = prev_x - ox;
                        let py = prev_y - oy;
                        if lx >= 0 && ly >= 0 && lx < PAINT_W as i32 && ly < PAINT_H as i32 {
                            if px >= 0 && py >= 0 && px < PAINT_W as i32 && py < PAINT_H as i32 {
                                self.paint_linea(px, py, lx, ly);
                            } else {
                                self.paint_punto(lx, ly);
                            }
                            cambio_escena = true;
                        }
                    }
                }
                cambio_escena
            }
            TIPO_TECLA_ABAJO => {
                if self.en_login {
                    match ev.codigo {
                        10 | 13 => self.verificar_login(),
                        8 => if self.pass_len > 0 { self.pass_len -= 1; },
                        32..=126 => if self.pass_len < self.pass_buf.len() - 1 {
                            self.pass_buf[self.pass_len] = ev.codigo as u8;
                            self.pass_len += 1;
                            self.login_error = false;
                        },
                        _ => {}
                    }
                    return true;
                }

                if ev.codigo == 27 {
                    self.mostrar_calendario = false;
                    self.mostrar_control = false;
                    self.mostrar_fondos = false;
                    return true;
                }
                if self.foco_actual() == Some(APP_TERMINAL) {
                    match ev.codigo {
                        10 | 13 => self.term_ejecutar(),
                        8 => if self.term_input_len > 0 { self.term_input_len -= 1; },
                        CTRL_V => {
                            for i in 0..self.portapapeles_len {
                                let b = self.portapapeles[i];
                                if b >= 32 && b <= 126 && self.term_input_len < self.term_input.len() - 1 {
                                    self.term_input[self.term_input_len] = b;
                                    self.term_input_len += 1;
                                }
                            }
                        }
                        32..=126 => if self.term_input_len < self.term_input.len() - 1 {
                            self.term_input[self.term_input_len] = ev.codigo as u8;
                            self.term_input_len += 1;
                        },
                        _ => {}
                    }
                } else if self.foco_actual() == Some(APP_EDITOR) {
                    match ev.codigo {
                        CTRL_S => self.exp_guardar(),
                        CTRL_C => self.exp_copiar_linea(),
                        CTRL_V => self.exp_pegar(),
                        10 | 13 => self.exp_insertar_byte(b'\n'),
                        9 => {
                            for _ in 0..4 { self.exp_insertar_byte(b' '); }
                        }
                        8 => self.exp_borrar_atras(),
                        TECLA_SUPR => self.exp_borrar_supr(),
                        TECLA_IZQ => if self.exp_cursor > 0 { self.exp_cursor -= 1; self.exp_asegurar_scroll(); },
                        TECLA_DER => if self.exp_cursor < self.exp_preview_len { self.exp_cursor += 1; self.exp_asegurar_scroll(); },
                        TECLA_ARRIBA => {
                            let (l, c) = self.exp_linea_col_cursor();
                            if l > 0 { self.exp_ir_a_linea_col(l - 1, c); }
                        }
                        TECLA_ABAJO => {
                            let (l, c) = self.exp_linea_col_cursor();
                            self.exp_ir_a_linea_col(l + 1, c);
                        }
                        TECLA_INICIO => {
                            let (l, _) = self.exp_linea_col_cursor();
                            self.exp_ir_a_linea_col(l, 0);
                        }
                        TECLA_FIN => {
                            let (l, _) = self.exp_linea_col_cursor();
                            self.exp_ir_a_linea_col(l, 999);
                        }
                        32..=126 => self.exp_insertar_byte(ev.codigo as u8),
                        _ => {}
                    }
                } else if self.foco_actual() == Some(APP_BROWSER) {
                    match ev.codigo {
                        10 | 13 => {
                            self.navegador.editando_url = false;
                            self.browser_navegar();
                        }
                        8 => if self.navegador.url_len > 0 {
                            self.navegador.url_len -= 1;
                            self.navegador.editando_url = true;
                        },
                        TECLA_ARRIBA => self.navegador.scroll = self.navegador.scroll.saturating_sub(2),
                        TECLA_ABAJO => self.navegador.scroll = self.navegador.scroll.saturating_add(2),
                        32..=126 => if self.navegador.url_len < self.navegador.url.len() - 1 {
                            if !self.navegador.editando_url {
                                self.navegador.editando_url = true;
                            }
                            let idx = self.navegador.url_len;
                            self.navegador.url[idx] = ev.codigo as u8;
                            self.navegador.url_len += 1;
                        },
                        _ => {}
                    }
                } else if self.foco_actual() == Some(APP_DOOM3D) {
                    match ev.codigo {
                        119 | 87 | TECLA_ARRIBA => self.doom.mover(1),
                        115 | 83 | TECLA_ABAJO => self.doom.mover(-1),
                        97 | 65 | TECLA_IZQ => self.doom.girar(-6),
                        100 | 68 | TECLA_DER => self.doom.girar(6),
                        32 => {
                            let hit = self.doom.disparar();
                            sonido_dsp(if hit { b"hit" } else { b"laser" });
                        }
                        _ => {}
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn on_clic_login(&mut self, x: i32, y: i32) {
        // Boton [ Entrar ] centrado en la tarjeta de login
        let cx = (W as i32) / 2;
        if x >= cx - 72 && x < cx + 72 && y >= 476 && y < 510 {
            self.verificar_login();
            return;
        }
        // Boton inferior derecho [ Consola TTY ]
        if x >= W as i32 - 134 && x < W as i32 - 18 && y >= H as i32 - 46 && y < H as i32 - 16 {
            self.salir_a_consola = true;
        }
    }

    fn on_clic_escritorio(&mut self, x: i32, y: i32) {
        // 1) Carrusel de fondos (si esta abierto)
        if self.mostrar_fondos {
            let fx = (W as i32 - 860) / 2;
            let fy = 246;
            // Selector de acento arriba del carrusel (4 colores centrados simetricamente)
            let cols = [COL_AZUL_ACENTO, COL_CIAN, COL_VERDE, COL_ROJO];
            for (i, &c) in cols.iter().enumerate() {
                let bx = (W as i32) / 2 - 48 + (i as i32) * 32;
                if x >= bx - 10 && x <= bx + 10 && y >= fy - 34 && y <= fy - 14 {
                    self.tema.acento = c;
                    self.guardar_tema_disco();
                    return;
                }
            }
            if x >= fx && x < fx + 860 && y >= fy && y < fy + 210 {
                for i in 0..4 {
                    let cx = fx + 20 + (i as i32) * 208;
                    if x >= cx && x < cx + 196 && y >= fy + 18 && y < fy + 188 {
                        self.cambiar_wallpaper_a = Some(i);
                        self.mostrar_fondos = false;
                        return;
                    }
                }
                return;
            }
            self.mostrar_fondos = false;
        }

        // 2) Panel de Centro de Control (arriba derecha)
        if self.mostrar_control {
            let px = W as i32 - 318;
            let py = 46;
            if x >= px && x < px + 306 && y >= py && y < py + 292 {
                // Slider Brillo (y = py + 54..72)
                if y >= py + 48 && y <= py + 76 && x >= px + 18 && x <= px + 288 {
                    let pct = (((x - (px + 18)) * 55) / 270 + 45).clamp(45, 100) as u32;
                    self.tema.brillo = pct;
                    return;
                }
                // Slider Opacidad (y = py + 98..126)
                if y >= py + 94 && y <= py + 122 && x >= px + 18 && x <= px + 288 {
                    let pct = (((x - (px + 18)) * 55) / 270 + 40).clamp(40, 95) as u32;
                    self.tema.opacidad = pct;
                    return;
                }
                // 2x2 tarjetas toggle: Blur, Tema Oscuro/Claro, Fondos, Bloquear
                if y >= py + 136 && y < py + 184 {
                    if x < px + 153 { self.tema.blur = !self.tema.blur; }
                    else { self.tema.oscuro = !self.tema.oscuro; }
                    self.guardar_tema_disco();
                    return;
                }
                if y >= py + 194 && y < py + 242 {
                    if x < px + 153 {
                        self.mostrar_control = false;
                        self.mostrar_fondos = true;
                    } else {
                        self.mostrar_control = false;
                        self.en_login = true;
                        self.pass_len = 0;
                    }
                    return;
                }
                if y >= py + 250 && y < py + 280 {
                    self.salir_a_consola = true;
                    return;
                }
                return;
            }
            self.mostrar_control = false;
        }

        // 3) Panel de Calendario (arriba centro)
        if self.mostrar_calendario {
            let cx = (W as i32 - 420) / 2;
            let cy = 46;
            if x >= cx && x < cx + 420 && y >= cy && y < cy + 250 {
                return;
            }
            self.mostrar_calendario = false;
        }

        // 4) Barra superior de 3 capsulas (y < 40)
        if y < 40 {
            // Capsula izquierda: workspaces 1, 2, 3 + Fondos
            for ws in 0..3u8 {
                let wx = 20 + (ws as i32) * 28;
                if x >= wx && x < wx + 24 {
                    self.workspace = ws;
                    if ws == 1 { self.monitor_refrescar(); }
                    if ws == 2 { self.exp_recargar(); }
                    return;
                }
            }
            if x >= 112 && x < 188 {
                self.mostrar_fondos = !self.mostrar_fondos;
                self.mostrar_calendario = false;
                self.mostrar_control = false;
                return;
            }
            // Capsula central: Reloj -> Calendario
            if x >= 540 && x < 740 {
                self.mostrar_calendario = !self.mostrar_calendario;
                self.mostrar_control = false;
                self.mostrar_fondos = false;
                return;
            }
            // Capsula derecha: Telemetria -> Centro de Control
            if x >= 980 && x < W as i32 - 10 {
                self.mostrar_control = !self.mostrar_control;
                self.mostrar_calendario = false;
                self.mostrar_fondos = false;
                return;
            }
            return;
        }

        // 5) Dock inferior flotante de 8 apps (y >= 648)
        if y >= 648 {
            let dock_w = 424;
            let dock_x = (W as i32 - dock_w) / 2;
            if x >= dock_x && x < dock_x + dock_w {
                let slot = ((x - dock_x - 14) / 50) as usize;
                match slot {
                    0 => self.abrir_o_enfocar(APP_TERMINAL),
                    1 => {
                        self.abrir_o_enfocar(APP_BROWSER);
                        self.mon_net_len = leer_todo(b"/proc/net\0", &mut self.mon_net);
                    }
                    2 => self.abrir_o_enfocar(APP_MONITOR),
                    3 => self.abrir_o_enfocar(APP_EDITOR),
                    4 => self.abrir_o_enfocar(APP_DOOM3D),
                    5 => {
                        self.mostrar_fondos = !self.mostrar_fondos;
                        self.mostrar_calendario = false;
                        self.mostrar_control = false;
                    }
                    6 => self.abrir_o_enfocar(APP_PAINT),
                    7 => { self.en_login = true; self.pass_len = 0; }
                    _ => {}
                }
                return;
            }
        }

        // 6) Ventanas del workspace actual
        for idx in (0..NUM_APPS).rev() {
            let app = self.orden_z[idx];
            let v = self.ventanas[app];
            if !v.abierta || v.workspace != self.workspace { continue; }
            if x >= v.x && x < v.x + v.w && y >= v.y && y < v.y + v.h {
                self.traer_al_frente(app);
                // Circulo rojo de cierre (arriba derecha)
                if (x - (v.x + v.w - 18)).abs() <= 9 && (y - (v.y + 18)).abs() <= 9 {
                    self.ventanas[app].abierta = false;
                    return;
                }
                // Zona de arrastre superior (34px)
                if y < v.y + 34 {
                    self.arrastrando = Some((app, x - v.x, y - v.y));
                    return;
                }
                if app == APP_EDITOR {
                    let lx = v.x + 18;
                    let ly = v.y + 44;
                    let lw = 296;
                    let rx = lx + lw + 16;
                    let rw = v.w - lw - 52;
                    // Boton [..Up]
                    if x >= v.x + 258 && x < v.x + 314 && y >= ly && y < ly + 24 {
                        self.exp_subir();
                        return;
                    }
                    // Botones toolbar del editor: [Guardar], [+ Nuevo], [Copiar]
                    if y >= ly + 8 && y < ly + 32 {
                        if x >= rx + rw - 246 && x < rx + rw - 170 {
                            self.exp_guardar();
                            return;
                        }
                        if x >= rx + rw - 162 && x < rx + rw - 88 {
                            self.exp_nuevo();
                            return;
                        }
                        if x >= rx + rw - 80 && x < rx + rw - 10 {
                            self.exp_copiar_linea();
                            return;
                        }
                    }
                    // Clic en arbol de archivos (panel izquierdo)
                    let list_y = ly + 38;
                    if x >= lx && x < lx + lw && y >= list_y && y < list_y + 440 {
                        let fila = ((y - list_y) / 26) as usize;
                        self.exp_clic(fila);
                        return;
                    }
                    // Clic en area de codigo: posicionar cursor de edicion exacto
                    if x >= rx + 40 && x < rx + rw - 8 && y >= ly + 40 && y < ly + v.h - 68 {
                        let fila_vis = ((y - (ly + 42)).max(0) / 19) as usize;
                        let col_vis = ((x - (rx + 46)).max(0) / (MONO_W as i32)) as usize;
                        self.exp_ir_a_linea_col(self.exp_scroll + fila_vis, col_vis);
                        return;
                    }
                } else if app == APP_BROWSER {
                    let bx = v.x + 18;
                    let bw = v.w - 36;
                    let by = v.y + 42;
                    let url_w = bw - 82;
                    // Barra de direcciones
                    if x >= bx && x < bx + url_w && y >= by && y < by + 30 {
                        self.navegador.editando_url = true;
                        return;
                    }
                    // Boton [Ir]
                    if x >= bx + url_w + 8 && x < bx + bw && y >= by && y < by + 30 {
                        self.navegador.editando_url = false;
                        self.browser_navegar();
                        return;
                    }
                    // Marcadores rapidos (4 capsulas)
                    let my = by + 38;
                    if y >= my && y < my + 24 {
                        let urls: [&[u8]; 4] = [
                            b"neo://home",
                            b"http://example.com/",
                            b"http://info.cern.ch/",
                            b"http://neverssl.com/",
                        ];
                        for (i, &u) in urls.iter().enumerate() {
                            let mx = bx + (i as i32) * 124;
                            if x >= mx && x < mx + 116 {
                                self.navegador.establecer_url(u);
                                self.navegador.editando_url = false;
                                self.browser_navegar();
                                return;
                            }
                        }
                    }
                } else if app == APP_DOOM3D {
                    let hit = self.doom.disparar();
                    sonido_dsp(if hit { b"hit" } else { b"laser" });
                    return;
                } else if app == APP_PAINT {
                    let py = v.y + 44;
                    if y >= py && y < py + 28 {
                        const COLS: [u32; 8] = [
                            0xF8FAFC, 0xF87171, 0xFB923C, 0xFACC15,
                            0x4ADE80, 0x38BDF8, 0x60A5FA, 0xC084FC,
                        ];
                        for (i, &c) in COLS.iter().enumerate() {
                            let cx = v.x + 34 + (i as i32) * 36;
                            if (x - cx).abs() <= 12 { self.paint_color = c; return; }
                        }
                        if x >= v.x + v.w - 94 && x < v.x + v.w - 22 { self.paint_limpiar(); return; }
                    }
                    let lx = x - (v.x + 22);
                    let ly = y - (v.y + 82);
                    if lx >= 0 && ly >= 0 && lx < PAINT_W as i32 && ly < PAINT_H as i32 {
                        self.paint_punto(lx, ly);
                    }
                }
                return;
            }
        }
    }

    fn browser_navegar(&mut self) {
        self.mon_net_len = leer_todo(b"/proc/net\0", &mut self.mon_net);
        let mut net_copy = [0u8; 256];
        let nl = self.mon_net_len;
        net_copy[..nl].copy_from_slice(&self.mon_net[..nl]);
        self.navegador.navegar(&net_copy[..nl], |url, dst| {
            let mut cmd = [0u8; 140];
            cmd[..4].copy_from_slice(b"get ");
            let n = url.len().min(132);
            cmd[4..4 + n].copy_from_slice(&url[..n]);
            red_comando(&cmd[..4 + n], dst)
        });
        self.mon_net_len = leer_todo(b"/proc/net\0", &mut self.mon_net);
    }

    // --- Renderizado ---

    fn dibujar(&self, l: &mut Lienzo) {
        if self.en_login {
            self.dibujar_login(l);
        } else {
            self.dibujar_escritorio(l);
        }
        l.aplicar_brillo(self.tema.brillo);
    }

    fn dibujar_login(&self, l: &mut Lienzo) {
        l.fondo(true); // Fondo suavemente desenfocado
        l.rect_alfa(0, 0, W as i32, H as i32, 0x06080D, 110);

        let cx = (W as i32) / 2;

        // Reloj grande suave arriba
        let mut hm = *b"00:00";
        fmt_2dig(self.reloj.hora, &mut hm, 0);
        fmt_2dig(self.reloj.minuto, &mut hm, 3);
        l.texto_disp_centrado(cx, 78, &hm, 6, 0xF8FAFC, 245);

        let mut fecha = [0u8; 32];
        let dl = self.reloj.nombre_dia_len;
        fecha[..dl].copy_from_slice(&self.reloj.nombre_dia[..dl]);
        fecha[dl] = b',';
        fecha[dl + 1] = b' ';
        fmt_2dig(self.reloj.dia, &mut fecha, dl + 2);
        fecha[dl + 4] = b' ';
        fecha[dl + 5..dl + 8].copy_from_slice(&self.reloj.nombre_mes[0..3]);
        fecha[dl + 8] = b' ';
        fecha[dl + 9..dl + 13].copy_from_slice(b"2026");
        l.texto_ui_centrado(cx, 136, &fecha[..dl + 13], COL_TEXTO_SUAVE);

        // Tarjeta de Login de cristal esmerilado con esquinas suaves (r=22)
        let cw = 356;
        let ch = 312;
        let lx = cx - cw / 2;
        let ly = 216;
        l.sombra_suave(lx, ly, cw, ch, 22);
        l.panel_cristal(lx, ly, cw, ch, 22, 0x12161E, 190, true, 0xFFFFFF, 42);

        // Avatar circular suave
        l.circulo_aa(cx, ly + 66, 36, 0x222936, 255);
        l.circulo_aa(cx, ly + 66, 34, self.tema.acento, 55);
        l.texto_disp_centrado(cx, ly + 43, b"G", 0, 0xFFFFFF, 245);

        // Nombre de usuario
        l.texto_ui_centrado(cx, ly + 114, b"guillermo", 0xFFFFFF);
        l.texto_ui_centrado(cx, ly + 134, b"neo_ws 1.0  |  3 GB RAM", COL_TEXTO_DIM);

        // Campo de contrasena redondeado
        let ix = cx - 126;
        let iy = ly + 168;
        let iw = 252;
        let ih = 38;
        l.rect_redondo(ix, iy, iw, ih, 12, 0x0A0D13, 210);
        let b_col = if self.login_error { COL_ROJO } else { self.tema.acento };
        l.borde_redondo(ix, iy, iw, ih, 12, b_col, 165);

        if self.pass_len == 0 {
            l.texto_ui(ix + 14, iy + 10, b"Contrasena (neo o Enter)...", COL_TEXTO_DIM);
        } else {
            for i in 0..self.pass_len.min(18) {
                l.circulo_aa(ix + 18 + (i as i32) * 12, iy + 19, 4, 0xF8FAFC, 245);
            }
        }

        if self.login_error {
            l.texto_ui_centrado(cx, iy + 46, b"Contrasena incorrecta (usa: neo)", COL_ROJO);
        }

        // Boton redondeado [ Iniciar sesion ]
        let bx = cx - 72;
        let by = 476;
        let hover_btn = self.mx >= bx && self.mx < bx + 144 && self.my >= by && self.my < by + 34;
        l.rect_redondo(bx, by, 144, 34, 17, self.tema.acento, if hover_btn { 255 } else { 225 });
        l.texto_ui_centrado(cx, by + 8, b"Iniciar sesion", 0xFFFFFF);

        // Capsula inferior derecha para salir a consola texto
        l.panel_cristal(W as i32 - 134, H as i32 - 46, 116, 30, 15, 0x10141C, 185, true, 0xFFFFFF, 35);
        l.texto_ui_centrado(W as i32 - 76, H as i32 - 39, b"Consola TTY", COL_TEXTO_SUAVE);
    }

    fn dibujar_escritorio(&self, l: &mut Lienzo) {
        l.fondo(false);

        // Reloj central minimalista (como 00:00 del video!) visible cuando el escritorio esta libre
        if !self.hay_ventana_en_workspace() && !self.mostrar_fondos {
            let cx = (W as i32) / 2;
            let cy = 265;
            // En fondos claros (Misty Peaks) oscurecemos el texto; en fondos oscuros lo dejamos claro.
            let col_big = if self.tema.wallpaper == 0 || self.tema.wallpaper == 1 { 0x14171D } else { 0xF8FAFC };
            let col_sub = if self.tema.wallpaper == 0 || self.tema.wallpaper == 1 { 0x2B313C } else { 0xD0D8E6 };

            l.texto_disp_centrado(
                cx,
                cy,
                &self.reloj.nombre_dia[..self.reloj.nombre_dia_len],
                14,
                col_big,
                235,
            );
            let mut linea_fecha = [0u8; 16];
            fmt_2dig(self.reloj.dia, &mut linea_fecha, 0);
            linea_fecha[2] = b' ';
            linea_fecha[3..6].copy_from_slice(&self.reloj.nombre_mes[0..3]);
            linea_fecha[6] = b' ';
            linea_fecha[7..11].copy_from_slice(b"2026");
            l.texto_ui_centrado(cx, cy + 56, &linea_fecha[..11], col_sub);

            let mut linea_hora = *b"- 00:00 -";
            fmt_2dig(self.reloj.hora, &mut linea_hora, 2);
            fmt_2dig(self.reloj.minuto, &mut linea_hora, 5);
            l.texto_ui_centrado(cx, cy + 78, &linea_hora, col_sub);
        }

        // Ventanas del workspace actual
        let foco = self.foco_actual();
        for &app in &self.orden_z {
            let v = self.ventanas[app];
            if v.abierta && v.workspace == self.workspace {
                self.dibujar_ventana(l, app, foco == Some(app));
            }
        }

        // Barra superior de 3 capsulas flotantes
        self.dibujar_barra_superior(l);

        // Dock inferior redondeado flotante
        self.dibujar_dock(l);

        // Paneles flotantes (Calendario, Centro de Control, Carrusel de Fondos)
        if self.mostrar_calendario { self.dibujar_calendario(l); }
        if self.mostrar_control { self.dibujar_control(l); }
        if self.mostrar_fondos { self.dibujar_carrusel_fondos(l); }
    }

    fn dibujar_barra_superior(&self, l: &mut Lienzo) {
        let tinte = self.tema.tinte();
        let bcol = self.tema.borde_col();
        let txt = self.tema.texto_principal();
        let txt2 = self.tema.texto_secundario();

        // 1) Capsula izquierda: Workspaces 1 2 3 + Fondos
        l.panel_cristal(12, 7, 178, 28, 14, tinte, 195, self.tema.blur, bcol, 34);
        let nums = [b"1", b"2", b"3"];
        for ws in 0..3u8 {
            let wx = 20 + (ws as i32) * 28;
            if ws == self.workspace {
                l.rect_redondo(wx, 11, 22, 20, 10, self.tema.acento, 240);
                l.texto_ui_centrado(wx + 11, 13, nums[ws as usize], 0xFFFFFF);
            } else {
                l.texto_ui_centrado(wx + 11, 13, nums[ws as usize], txt2);
            }
        }
        l.rect_alfa(106, 13, 1, 16, bcol, 36);
        let hov_f = self.my < 38 && self.mx >= 112 && self.mx < 186;
        if hov_f || self.mostrar_fondos {
            l.rect_redondo(112, 11, 70, 20, 10, bcol, 28);
        }
        l.texto_ui_centrado(147, 13, b"Fondos", txt);

        // 2) Capsula central: Fecha y reloj real (clickable -> Calendario)
        let cx = (W as i32) / 2;
        l.panel_cristal(cx - 94, 7, 188, 28, 14, tinte, 195, self.tema.blur, bcol, 34);
        let mut reloj_str = *b"27 Nov  00:00:00";
        fmt_2dig(self.reloj.dia, &mut reloj_str, 0);
        reloj_str[3..6].copy_from_slice(&self.reloj.nombre_mes[0..3]);
        fmt_2dig(self.reloj.hora, &mut reloj_str, 8);
        fmt_2dig(self.reloj.minuto, &mut reloj_str, 11);
        fmt_2dig(self.reloj.segundo, &mut reloj_str, 14);
        l.texto_ui_centrado(cx, 13, &reloj_str, txt);

        // 3) Capsula derecha: Telemetria + Centro de Control
        let rx = W as i32 - 298;
        l.panel_cristal(rx, 7, 286, 28, 14, tinte, 195, self.tema.blur, bcol, 34);
        l.circulo_aa(rx + 18, 21, 4, COL_VERDE, 255);
        l.texto_ui(rx + 28, 13, b"4xCPU", txt2);
        l.texto_ui(rx + 86, 13, b"3.0G", txt);
        l.texto_ui(rx + 130, 13, b"NET", COL_VERDE);
        l.rect_alfa(rx + 170, 13, 1, 16, bcol, 36);
        l.texto_ui(rx + 182, 13, b"Control", txt);
        l.circulo_aa(rx + 266, 21, 5, self.tema.acento, 240);
    }

    fn dibujar_dock(&self, l: &mut Lienzo) {
        let dock_w = 424;
        let dock_h = 58;
        let dx = (W as i32 - dock_w) / 2;
        let dy = H as i32 - dock_h - 10;

        l.sombra_suave(dx, dy, dock_w, dock_h, 18);
        l.panel_cristal(dx, dy, dock_w, dock_h, 18, self.tema.tinte(), 188, self.tema.blur, self.tema.borde_col(), 42);

        let cols = [0x1F2937, 0x2563EB, 0x0EA5E9, 0xD97706, 0xDC2626, 0x8B5CF6, 0xEC4899, 0x475569];
        let etqs: [&[u8]; 8] = [b">_", b"WEB", b"TOP", b"</>", b"3D", b"IMG", b"ART", b"LCK"];
        let tooltips: [&[u8]; 8] = [
            b"Terminal",
            b"NeoWeb (Internet)",
            b"btop Monitor",
            b"Editor & NeoFS",
            b"NeoDoom 3D",
            b"Fondos",
            b"Paint",
            b"Bloquear",
        ];

        for i in 0..8 {
            let ix = dx + 16 + (i as i32) * 50;
            let iy = dy + 7;
            let hover = self.mx >= ix && self.mx < ix + 38 && self.my >= iy && self.my < iy + 38;
            let lift = if hover { 4 } else { 0 };

            l.rect_redondo(ix, iy - lift, 38, 38, 11, cols[i], 240);
            l.borde_redondo(ix, iy - lift, 38, 38, 11, 0xFFFFFF, if hover { 110 } else { 42 });
            l.texto_ui_centrado(ix + 19, iy + 10 - lift, etqs[i], 0xFFFFFF);

            // Punto activo debajo si la app esta abierta
            let abierta = match i {
                0 => self.ventanas[APP_TERMINAL].abierta,
                1 => self.ventanas[APP_BROWSER].abierta,
                2 => self.ventanas[APP_MONITOR].abierta,
                3 => self.ventanas[APP_EDITOR].abierta,
                4 => self.ventanas[APP_DOOM3D].abierta,
                6 => self.ventanas[APP_PAINT].abierta,
                _ => false,
            };
            if abierta {
                l.circulo_aa(ix + 19, dy + dock_h - 6, 2, self.tema.acento, 255);
            }

            if hover {
                let tw = Lienzo::ancho_ui(tooltips[i]) + 18;
                let tx = (ix + 19 - tw / 2).clamp(12, W as i32 - tw - 12);
                l.panel_cristal(tx, dy - 30, tw, 24, 10, 0x10141D, 220, true, 0xFFFFFF, 40);
                l.texto_ui_centrado(tx + tw / 2, dy - 26, tooltips[i], 0xFFFFFF);
            }
        }
    }

    fn dibujar_calendario(&self, l: &mut Lienzo) {
        let w = 420;
        let h = 250;
        let x = (W as i32 - w) / 2;
        let y = 46;
        let txt = self.tema.texto_principal();
        let txt2 = self.tema.texto_secundario();

        l.sombra_suave(x, y, w, h, 16);
        l.panel_cristal(x, y, w, h, 16, self.tema.tinte(), 218, self.tema.blur, self.tema.borde_col(), 45);

        // Mitad izquierda: Notificaciones (estilo 00:08 del video!)
        l.rect_alfa(x + 178, y + 16, 1, h - 32, self.tema.borde_col(), 30);
        l.circulo_aa(x + 89, y + 98, 22, self.tema.borde_col(), 20);
        l.texto_ui_centrado(x + 89, y + 90, b"OK", self.tema.acento);
        l.texto_ui_centrado(x + 89, y + 132, b"Sin notificaciones", txt2);
        l.texto_ui_centrado(x + 89, y + 154, b"neo_ws 2.0 activo", COL_TEXTO_DIM);

        // Mitad derecha: Calendario del mes real
        let gx = x + 196;
        let mut titulo = *b"NOV 2026";
        titulo[0..3].copy_from_slice(&self.reloj.nombre_mes[0..3]);
        l.texto_ui(gx + 6, y + 16, &titulo, txt);

        let cab: [&[u8]; 7] = [b"D", b"L", b"M", b"X", b"J", b"V", b"S"];
        for (i, &c) in cab.iter().enumerate() {
            l.texto_ui_centrado(gx + 14 + (i as i32) * 30, y + 44, c, COL_TEXTO_DIM);
        }

        // Calculamos en que dia de la semana empezo el dia 1 del mes actual
        let inicio_mes = (self.reloj.dia_sem as i32 - ((self.reloj.dia as i32 - 1) % 7)).rem_euclid(7);
        let dias_mes = match self.reloj.mes {
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
        for d in 1..=dias_mes {
            let cel = inicio_mes + d - 1;
            let col_i = cel % 7;
            let row_i = cel / 7;
            let dx = gx + 14 + col_i * 30;
            let dy = y + 72 + row_i * 28;
            if d as u32 == self.reloj.dia {
                l.circulo_aa(dx, dy + 8, 12, self.tema.acento, 245);
            }
            let mut db = [0u8; 3];
            if d < 10 {
                db[0] = b'0' + d as u8;
                l.texto_ui_centrado(dx, dy, &db[..1], if d as u32 == self.reloj.dia { 0xFFFFFF } else { txt });
            } else {
                db[0] = b'0' + (d / 10) as u8;
                db[1] = b'0' + (d % 10) as u8;
                l.texto_ui_centrado(dx, dy, &db[..2], if d as u32 == self.reloj.dia { 0xFFFFFF } else { txt });
            }
        }
    }

    fn dibujar_control(&self, l: &mut Lienzo) {
        let w = 306;
        let h = 292;
        let x = W as i32 - w - 12;
        let y = 46;
        let txt = self.tema.texto_principal();
        let txt2 = self.tema.texto_secundario();

        l.sombra_suave(x, y, w, h, 16);
        l.panel_cristal(x, y, w, h, 16, self.tema.tinte(), 220, self.tema.blur, self.tema.borde_col(), 45);
        l.texto_ui(x + 18, y + 14, b"Centro de Control", txt);

        // Slider Brillo
        l.texto_ui(x + 18, y + 40, b"Brillo de pantalla", txt2);
        l.rect_redondo(x + 18, y + 62, 270, 8, 4, self.tema.borde_col(), 35);
        let bw = (((self.tema.brillo - 45) * 270) / 55) as i32;
        l.rect_redondo(x + 18, y + 62, bw.max(12), 8, 4, self.tema.acento, 245);
        l.circulo_aa(x + 18 + bw.clamp(6, 264), y + 66, 7, 0xFFFFFF, 255);

        // Slider Opacidad de cristal
        l.texto_ui(x + 18, y + 84, b"Opacidad de ventanas", txt2);
        l.rect_redondo(x + 18, y + 106, 270, 8, 4, self.tema.borde_col(), 35);
        let ow = (((self.tema.opacidad - 40) * 270) / 55) as i32;
        l.rect_redondo(x + 18, y + 106, ow.max(12), 8, 4, self.tema.acento, 245);
        l.circulo_aa(x + 18 + ow.clamp(6, 264), y + 110, 7, 0xFFFFFF, 255);

        // 4 tarjetas redondeadas estilo GNOME 47 (como 00:06 del video!)
        let btn = |l: &mut Lienzo, bx: i32, by: i32, activo: bool, titulo: &[u8], sub: &[u8]| {
            let col = if activo { self.tema.acento } else { 0x252C38 };
            l.rect_redondo(bx, by, 130, 48, 12, col, if activo { 235 } else { 190 });
            l.texto_ui(bx + 12, by + 8, titulo, 0xFFFFFF);
            l.texto_ui(bx + 12, by + 26, sub, if activo { 0xE2E8F0 } else { COL_TEXTO_SUAVE });
        };

        btn(l, x + 18, y + 136, self.tema.blur, b"Blur Cristal", if self.tema.blur { b"Activado" } else { b"Desactivado" });
        btn(l, x + 158, y + 136, self.tema.oscuro, b"Modo Oscuro", if self.tema.oscuro { b"Oscuro" } else { b"Claro" });
        btn(l, x + 18, y + 194, false, b"Fondos", b"Cambiar tema");
        btn(l, x + 158, y + 194, false, b"Bloquear", b"Ir al Login");

        // Boton Consola TTY
        l.rect_redondo(x + 18, y + 252, 270, 28, 10, COL_ROJO, 180);
        l.texto_ui_centrado(x + 153, y + 258, b"Salir a consola de texto (TTY)", 0xFFFFFF);
    }

    fn dibujar_carrusel_fondos(&self, l: &mut Lienzo) {
        let w = 860;
        let h = 210;
        let x = (W as i32 - w) / 2;
        let y = 246;
        let cx = (W as i32) / 2;

        // Capsula superior de seleccion de color de acento (simetrica sobre cx)
        l.panel_cristal(cx - 76, y - 42, 152, 32, 16, self.tema.tinte(), 210, true, 0xFFFFFF, 40);
        let cols = [COL_AZUL_ACENTO, COL_CIAN, COL_VERDE, COL_ROJO];
        for (i, &c) in cols.iter().enumerate() {
            let bx = cx - 48 + (i as i32) * 32;
            l.circulo_aa(bx, y - 26, 9, c, 255);
            if c == self.tema.acento {
                l.circulo_aa(bx, y - 26, 3, 0xFFFFFF, 255);
            }
        }

        // Panel del carrusel con 4 tarjetas visuales redondeadas con mini-silueta real
        l.sombra_suave(x, y, w, h, 20);
        l.panel_cristal(x, y, w, h, 20, self.tema.tinte(), 215, true, self.tema.borde_col(), 48);

        let nombres: [&[u8]; 4] = [b"Silk Waves", b"Misty Peaks", b"Obsidian Hex", b"Snow Leopard"];

        for i in 0..4 {
            let tx = x + 22 + (i as i32) * 208;
            let ty = y + 18;
            let tw = 192;
            let th = 148;
            let r = 12i32;
            let sel = self.tema.wallpaper == i;

            for py in 0..th {
                for px in 0..tw {
                    // Recorte limpio a las esquinas redondeadas de radio r
                    let dx = if px < r { r - px } else if px >= tw - r { px - (tw - 1 - r) } else { 0 };
                    let dy = if py < r { r - py } else if py >= th - r { py - (th - 1 - r) } else { 0 };
                    if dx > 0 && dy > 0 && dx * dx + dy * dy > r * r {
                        continue;
                    }
                    let c = match i {
                        0 => {
                            // Silk Waves mini-ondas suaves
                            let w1 = 84 + ((px * 3 / 5 + 20) % 64 - 32).abs() / 2 - (px / 10);
                            let w2 = 108 - ((px * 2 / 3) % 52 - 26).abs() / 2;
                            if py > w2 {
                                mezclar(0x454A52, 0x16191F, (((py - w2) * 255) / (th - w2).max(1)) as u32)
                            } else if py > w1 {
                                mezclar(0x9BA2AC, 0x353A42, (((py - w1) * 255) / (w2 - w1).max(1)) as u32)
                            } else {
                                mezclar(0x888F98, 0x626872, ((py * 255) / th) as u32)
                            }
                        }
                        1 => {
                            // Misty Peaks mini-montanas en capas
                            let p1 = 44 + (px - 108).abs() * 3 / 5;
                            let p2 = 76 + (px - 70).abs() * 2 / 5;
                            let p3 = 112 + (px - 140).abs() / 5;
                            if py > p3 { 0x1F232A }
                            else if py > p2 { mezclar(0x525862, 0x2F343D, (((py - p2) * 255) / 50).min(255) as u32) }
                            else if py > p1 { mezclar(0x8A919B, 0x565C66, (((py - p1) * 255) / 60).min(255) as u32) }
                            else { mezclar(0xC8CDD5, 0xADB3BC, ((py * 255) / th) as u32) }
                        }
                        2 => {
                            // Obsidian Hex con patron geometrico cian sutil
                            let base = mezclar(0x161B26, 0x090C12, ((py * 255) / th) as u32);
                            let gx = (px + py / 2) % 24;
                            let gy = (py + px / 2) % 24;
                            if gx == 0 || gy == 0 { mezclar(base, 0x38BDF8, 42) } else { base }
                        }
                        _ => {
                            // Snow Leopard invierno nocturno + nieve suave
                            let base = mezclar(0x152030, 0x4E647D, ((py * 255) / th) as u32);
                            let snow = ((px * 37 + py * 73) ^ (px * 13)) & 255;
                            if py > 106 {
                                mezclar(0x6D8298, 0x2C3A4A, (((py - 106) * 255) / 42) as u32)
                            } else if snow < 4 {
                                0xE2E8F0
                            } else {
                                base
                            }
                        }
                    };
                    l.poner(tx + px, ty + py, c);
                }
            }

            l.borde_redondo(tx, ty, tw, th, r, if sel { self.tema.acento } else { 0xFFFFFF }, if sel { 255 } else { 60 });
            if sel {
                l.borde_redondo(tx - 1, ty - 1, tw + 2, th + 2, r + 1, self.tema.acento, 200);
            }
            l.texto_ui_centrado(tx + tw / 2, ty + th + 12, nombres[i], if sel { 0xFFFFFF } else { COL_TEXTO_SUAVE });
        }
    }

    fn dibujar_ventana(&self, l: &mut Lienzo, app: usize, activa: bool) {
        let v = self.ventanas[app];
        let r = 14;
        l.sombra_suave(v.x, v.y, v.w, v.h, r);
        l.panel_cristal(
            v.x, v.y, v.w, v.h, r,
            self.tema.tinte(),
            self.tema.alfa_ventana(),
            self.tema.blur,
            self.tema.borde_col(),
            if activa { 54 } else { 26 },
        );

        // Barra superior fina con linea divisora sutil + circulo rojo suave para cerrar
        let titulo: &[u8] = match app {
            APP_TERMINAL => b"guillermo@neo_ws  -  Terminal",
            APP_MONITOR => b"btop  -  Monitor del Sistema",
            APP_EDITOR => b"Editor de Codigo & NeoFS (Disco Persistente)",
            APP_BROWSER => b"NeoWeb 2.0  -  Navegador Internet (Intel e1000)",
            APP_DOOM3D => b"NeoDoom 3D  -  Motor Raycaster Nativo (WASD + Espacio)",
            _ => b"Paint  -  Lienzo Suave",
        };
        l.texto_ui_centrado(v.x + v.w / 2, v.y + 9, titulo, self.tema.texto_secundario());
        l.circulo_aa(v.x + v.w - 18, v.y + 18, 6, COL_ROJO, 235);
        l.circulo_aa(v.x + v.w - 36, v.y + 18, 6, COL_AMARILLO, 180);
        l.circulo_aa(v.x + v.w - 54, v.y + 18, 6, COL_VERDE, 180);
        l.rect_alfa(v.x + 12, v.y + 34, v.w - 24, 1, self.tema.borde_col(), 24);

        match app {
            APP_TERMINAL => self.dibujar_terminal(l, v),
            APP_MONITOR => self.dibujar_btop(l, v),
            APP_EDITOR => self.dibujar_editor(l, v),
            APP_BROWSER => self.navegador.dibujar(
                l,
                v.x,
                v.y,
                v.w,
                v.h,
                self.tema.acento,
                self.tema.borde_col(),
                self.tema.texto_principal(),
                self.tema.texto_secundario(),
            ),
            APP_DOOM3D => self.dibujar_doom3d(l, v),
            _ => self.dibujar_paint(l, v),
        }
    }

    fn dibujar_doom3d(&self, l: &mut Lienzo, v: Ventana) {
        let ox = v.x + 18;
        let oy = v.y + 44;
        self.doom.dibujar(l, ox, oy, self.tema.acento);
        l.borde_redondo(ox - 1, oy - 1, doom3d::VIEW_W as i32 + 2, doom3d::VIEW_H as i32 + 2, 8, self.tema.borde_col(), 45);
        let py = oy + doom3d::VIEW_H as i32 + 12;
        l.texto_ui(ox, py, b"WASD / Flechas: Mover y girar  |  Espacio / Clic: Disparar", self.tema.texto_secundario());
        let mut sc = *b"Puntos: 0000";
        let p = self.doom.puntaje;
        sc[8] = b'0' + ((p / 1000) % 10) as u8;
        sc[9] = b'0' + ((p / 100) % 10) as u8;
        sc[10] = b'0' + ((p / 10) % 10) as u8;
        sc[11] = b'0' + (p % 10) as u8;
        l.texto_ui(ox + doom3d::VIEW_W as i32 - 96, py, &sc, COL_VERDE);
    }

    fn dibujar_terminal(&self, l: &mut Lienzo, v: Ventana) {
        let txt = self.tema.texto_principal();
        let txt2 = self.tema.texto_secundario();
        let mut y = v.y + 44;

        if self.mostrar_fastfetch {
            // Mascota pixel-art limpia (escala 3 => 96x108)
            let mx = v.x + 24;
            let my = y + 10;
            for py in 0..MASCOTA_H {
                for px in 0..MASCOTA_W {
                    let c = MASCOTA[py * MASCOTA_W + px];
                    if c != 0 {
                        l.rect(mx + (px as i32) * 3, my + (py as i32) * 3, 3, 3, c);
                    }
                }
            }
            let tx = v.x + 146;
            l.texto_mono(tx, y, b"guillermo@neo_ws", 40, self.tema.acento);
            l.rect_alfa(tx, y + 18, 220, 1, self.tema.borde_col(), 40);
            l.texto_mono(tx, y + 24, b"OS:      neo_ws 2.0 (Linux ABI)", 46, txt);
            l.texto_mono(tx, y + 42, b"Kernel:  Linux 6.1.0-neo_ws x86_64", 46, txt);
            l.texto_mono(tx, y + 60, b"Net/Snd: Intel e1000 + AC97 + NeoFS", 46, COL_CIAN);
            l.texto_mono(tx, y + 78, b"GPU:     Bochs BGA 1280x720 0-lag", 46, txt);
            l.texto_mono(tx, y + 96, b"RAM:     ", 10, txt);
            l.texto_mono(tx + 81, y + 96, &self.mon_mem[..self.mon_mem_len], 14, COL_AMARILLO);
            l.texto_mono(tx, y + 114, b"Shell:   41 syscalls | musl/std OK", 46, COL_CIAN);
            l.texto_mono(tx, y + 132, b"Uptime:  ", 10, txt2);
            l.texto_mono(tx + 81, y + 132, &self.mon_uptime[..self.mon_uptime_len], 30, COL_VERDE);

            // Barras de color neofetch
            let pal = [0x1E293B, COL_ROJO, COL_VERDE, COL_AMARILLO, COL_AZUL_ACENTO, COL_PURPURA, COL_CIAN, 0xF8FAFC];
            for (i, &c) in pal.iter().enumerate() {
                l.rect_redondo(tx + (i as i32) * 26, y + 156, 22, 10, 4, c, 240);
            }
            y += 182;
        }

        let max_y = v.y + v.h - 38;
        for i in 0..self.term_count {
            if y + (MONO_H as i32) > max_y { break; }
            l.texto_mono(v.x + 18, y, &self.term_lineas[i][..self.term_len[i]], TERM_COLS, self.term_colores[i]);
            y += (MONO_H + 1) as i32;
        }

        // Prompt suave inferior
        let py = (y + 6).min(v.y + v.h - 30);
        l.texto_mono(v.x + 18, py, b"neo_ws ", 10, COL_VERDE);
        l.texto_mono(v.x + 81, py, &self.cwd_cache[..self.cwd_len], 14, COL_CIAN);
        let fin = v.x + 81 + (self.cwd_len.min(14) * MONO_W) as i32;
        l.texto_mono(fin, py, b" > ", 5, COL_TEXTO_DIM);
        let ix = fin + 27;
        l.texto_mono(ix, py, &self.term_input[..self.term_input_len], 46, txt);
        let cx = ix + (self.term_input_len.min(46) * MONO_W) as i32;
        l.rect_redondo(cx, py + 1, 7, 14, 2, self.tema.acento, 240);
    }

    /// Monitor estilo btop (inspirado en 00:16 del video!) con grafica CPU en vivo + barra RAM 3GB + procesos.
    fn dibujar_btop(&self, l: &mut Lienzo, v: Ventana) {
        let txt = self.tema.texto_principal();
        let txt2 = self.tema.texto_secundario();
        let bx = v.x + 18;
        let bw = v.w - 36;

        // Tarjeta 1: CPU + grafica de barras en vivo
        let cy = v.y + 46;
        l.rect_redondo(bx, cy, bw, 152, 12, 0x0A0D14, 150);
        l.borde_redondo(bx, cy, bw, 152, 12, self.tema.borde_col(), 34);
        l.texto_ui(bx + 14, cy + 10, b"CPU  |  4 vCPUs x86_64 (WHPX)", COL_CIAN);
        l.texto_ui(bx + bw - 92, cy + 10, b"60 Hz  OK", COL_VERDE);

        let gx = bx + 16;
        let gy = cy + 136;
        for i in 0..self.cpu_hist.len() {
            let idx = (self.cpu_hist_idx + i) % self.cpu_hist.len();
            let val = (self.cpu_hist[idx] as i32).clamp(4, 90);
            let bh = (val * 92) / 90;
            let col = if val > 42 { self.tema.acento } else { COL_CIAN };
            l.rect_redondo(gx + (i as i32) * 12, gy - bh, 8, bh, 3, col, 220);
        }

        // Tarjeta 2: Memoria 3 GB + Disco ATA
        let my = cy + 164;
        l.rect_redondo(bx, my, bw, 126, 12, 0x0A0D14, 150);
        l.borde_redondo(bx, my, bw, 126, 12, self.tema.borde_col(), 34);
        l.texto_ui(bx + 14, my + 10, b"Memoria RAM", COL_AMARILLO);
        l.texto_mono(bx + 120, my + 10, &self.mon_mem[..self.mon_mem_len], 16, txt);
        l.texto_ui(bx + 250, my + 10, b"(Heap Kernel: 16 MiB)", txt2);

        // Barra suave de uso de RAM
        l.rect_redondo(bx + 14, my + 34, bw - 28, 12, 6, 0x1F2736, 220);
        l.rect_redondo(bx + 14, my + 34, 48, 12, 6, COL_VERDE, 235);

        l.texto_ui(bx + 14, my + 58, b"Disco virtual (/proc/disk):", COL_CIAN);
        dibujar_mono_multi(l, bx + 14, my + 78, &self.mon_disk[..self.mon_disk_len], 60, 2, txt2);

        // Tarjeta 3: Procesos en vivo (/proc/ps)
        let py = my + 138;
        let ph = v.y + v.h - py - 18;
        l.rect_redondo(bx, py, bw, ph, 12, 0x0A0D14, 150);
        l.borde_redondo(bx, py, bw, ph, 12, self.tema.borde_col(), 34);
        l.texto_ui(bx + 14, py + 10, b"Procesos activos (/proc/ps)", COL_PURPURA);
        dibujar_mono_multi(l, bx + 14, py + 34, &self.mon_ps[..self.mon_ps_len], 60, 10, txt);
    }

    /// Explorador + Visor de Codigo con resaltado de sintaxis (inspirado en 00:20 del video!).
    fn dibujar_editor(&self, l: &mut Lienzo, v: Ventana) {
        let txt = self.tema.texto_principal();
        let txt2 = self.tema.texto_secundario();

        // Panel izquierdo: Arbol de archivos
        let lx = v.x + 18;
        let ly = v.y + 44;
        let lw = 296;
        l.rect_redondo(lx, ly, lw - 64, 24, 8, 0x0A0D14, 180);
        l.texto_ui_lim(lx + 10, ly + 3, &self.exp_dir[..self.exp_dir_len], lw - 80, COL_CIAN);

        l.rect_redondo(lx + lw - 56, ly, 56, 24, 8, 0x242C3B, 220);
        l.texto_ui_centrado(lx + lw - 28, ly + 3, b"..Up", 0xFFFFFF);

        let list_y = ly + 36;
        let list_h = v.h - 98;
        l.rect_redondo(lx, list_y, lw, list_h, 12, 0x0A0D14, 155);
        l.borde_redondo(lx, list_y, lw, list_h, 12, self.tema.borde_col(), 30);

        for i in 0..self.exp_count.min(16) {
            let e = &self.exp_entradas[i];
            let ry = list_y + 8 + (i as i32) * 26;
            let hov = self.mx >= lx + 6 && self.mx < lx + lw - 6 && self.my >= ry && self.my < ry + 24;
            if hov {
                l.rect_redondo(lx + 6, ry, lw - 12, 24, 7, self.tema.acento, 55);
            }
            if e.es_dir {
                l.circulo_aa(lx + 20, ry + 12, 4, COL_AMARILLO, 240);
                l.texto_ui_lim(lx + 32, ry + 3, &e.nombre[..e.len], lw - 44, COL_CIAN);
            } else {
                l.circulo_aa(lx + 20, ry + 12, 3, COL_TEXTO_DIM, 200);
                l.texto_ui_lim(lx + 32, ry + 3, &e.nombre[..e.len], lw - 44, txt);
            }
        }

        // Panel derecho: Editor de codigo REAL con resaltado, cursor y guardado en NeoFS
        let rx = lx + lw + 16;
        let rw = v.w - lw - 52;
        l.rect_redondo(rx, ly, rw, v.h - 62, 12, 0x0A0D14, 175);
        l.borde_redondo(rx, ly, rw, v.h - 62, 12, self.tema.borde_col(), 34);

        // Pestana del archivo abierto (+ indicador * si hay cambios sin guardar)
        l.rect_redondo(rx + 10, ly + 8, 286, 24, 8, 0x1A2130, 220);
        let mut tab = [0u8; 68];
        let tl = self.exp_sel_len.min(64);
        tab[..tl].copy_from_slice(&self.exp_sel_nombre[..tl]);
        let tl = if self.exp_modificado {
            tab[tl] = b' ';
            tab[tl + 1] = b'*';
            tl + 2
        } else {
            tl
        };
        l.texto_ui_lim(
            rx + 20,
            ly + 11,
            &tab[..tl],
            266,
            if self.exp_modificado { COL_AMARILLO } else { COL_VERDE },
        );

        // Notificacion breve "Guardado en disco!"
        if self.exp_msg_guardado > 0 {
            l.rect_redondo(rx + 306, ly + 8, 150, 24, 8, COL_VERDE, 45);
            l.texto_ui_centrado(rx + 381, ly + 11, b"Guardado en disco!", COL_VERDE);
        }

        // Botones de accion: [Guardar] [+ Nuevo] [Copiar]
        let col_save = if self.exp_modificado { COL_VERDE } else { self.tema.acento };
        l.rect_redondo(rx + rw - 246, ly + 8, 76, 24, 8, col_save, 225);
        l.texto_ui_centrado(rx + rw - 208, ly + 11, b"Guardar", 0xFFFFFF);

        l.rect_redondo(rx + rw - 162, ly + 8, 74, 24, 8, 0x242C3B, 225);
        l.texto_ui_centrado(rx + rw - 125, ly + 11, b"+ Nuevo", 0xFFFFFF);

        l.rect_redondo(rx + rw - 80, ly + 8, 70, 24, 8, 0x242C3B, 225);
        l.texto_ui_centrado(rx + rw - 45, ly + 11, b"Copiar", 0xFFFFFF);

        let (cur_linea, cur_col) = self.exp_linea_col_cursor();
        let mut cy = ly + 42;
        let mut ini = 0usize;
        let mut idx_linea = 0usize;
        let max_y = ly + v.h - 86;
        for i in 0..=self.exp_preview_len {
            if i == self.exp_preview_len || self.exp_preview[i] == b'\n' {
                if idx_linea >= self.exp_scroll {
                    if cy > max_y { break; }
                    let num_linea = (idx_linea + 1) as u32;
                    if idx_linea == cur_linea {
                        l.rect_alfa(rx + 8, cy - 2, rw - 16, 19, self.tema.acento, 28);
                    }
                    let mut nb = *b"   ";
                    if num_linea >= 100 {
                        nb[0] = b'0' + ((num_linea / 100) % 10) as u8;
                    }
                    if num_linea >= 10 {
                        nb[1] = b'0' + ((num_linea / 10) % 10) as u8;
                    }
                    nb[2] = b'0' + (num_linea % 10) as u8;
                    l.texto_mono(
                        rx + 10,
                        cy,
                        &nb,
                        3,
                        if idx_linea == cur_linea { txt } else { COL_TEXTO_DIM },
                    );
                    let slice = &self.exp_preview[ini..i];
                    let col_linea = color_sintaxis(slice, txt, txt2);
                    l.texto_mono(rx + 46, cy, slice, 82, col_linea);
                    if idx_linea == cur_linea {
                        let cx = rx + 46 + (cur_col.min(81) * MONO_W) as i32;
                        l.rect_redondo(cx, cy, 2, 16, 1, self.tema.acento, 255);
                    }
                    cy += 19;
                }
                idx_linea += 1;
                ini = i + 1;
            }
        }

        // Barra inferior de estado del editor
        l.texto_ui(
            rx + 14,
            ly + v.h - 84,
            b"Ctrl+S Guardar  |  Ctrl+C Copiar  |  Ctrl+V Pegar  |  Rueda: Scroll",
            COL_TEXTO_DIM,
        );
    }

    fn dibujar_paint(&self, l: &mut Lienzo, v: Ventana) {
        const COLS: [u32; 8] = [
            0xF8FAFC, 0xF87171, 0xFB923C, 0xFACC15,
            0x4ADE80, 0x38BDF8, 0x60A5FA, 0xC084FC,
        ];
        let py = v.y + 44;
        for (i, &c) in COLS.iter().enumerate() {
            let cx = v.x + 34 + (i as i32) * 36;
            l.circulo_aa(cx, py + 14, 11, c, 255);
            if c == self.paint_color {
                l.circulo_aa(cx, py + 14, 4, 0x0A0D14, 255);
            }
        }
        l.rect_redondo(v.x + v.w - 94, py + 2, 72, 24, 10, 0x242C3B, 220);
        l.texto_ui_centrado(v.x + v.w - 58, py + 5, b"Limpiar", 0xFFFFFF);

        let cx = v.x + 22;
        let cy = v.y + 82;
        l.borde_redondo(cx - 1, cy - 1, PAINT_W as i32 + 2, PAINT_H as i32 + 2, 10, self.tema.borde_col(), 50);
        if !self.paint_buf.is_null() {
            for row in 0..PAINT_H {
                let sy = cy + row as i32;
                if sy < 0 || sy >= H as i32 { continue; }
                let dst_base = (sy as usize) * W;
                let src_base = row * PAINT_W;
                for col in 0..PAINT_W {
                    let sx = cx + col as i32;
                    if sx >= 0 && sx < W as i32 {
                        unsafe { l.px[dst_base + sx as usize] = *self.paint_buf.add(src_base + col); }
                    }
                }
            }
        }
    }
}

fn color_sintaxis(linea: &[u8], normal: u32, suave: u32) -> u32 {
    let t = recortar(linea);
    if t.is_empty() { return normal; }
    if t[0] == b'#' { return COL_TEXTO_DIM; }
    if t.starts_with(b"def ") || t.starts_with(b"from ") || t.starts_with(b"import ") || t.starts_with(b"return ") {
        return COL_PURPURA;
    }
    if t.starts_with(b"for ") || t.starts_with(b"if ") || t.starts_with(b"else") {
        return COL_CIAN;
    }
    if t.contains(&b'"') { return COL_VERDE; }
    if t.contains(&b'=') { return COL_AMARILLO; }
    suave
}

fn dibujar_mono_multi(
    l: &mut Lienzo,
    x: i32,
    mut y: i32,
    datos: &[u8],
    max_cols: usize,
    max_filas: usize,
    col: u32,
) {
    let mut ini = 0;
    let mut filas = 0;
    for i in 0..=datos.len() {
        if i == datos.len() || datos[i] == b'\n' {
            if i > ini && filas < max_filas {
                l.texto_mono(x, y, &datos[ini..i], max_cols, col);
                y += 17;
                filas += 1;
            }
            ini = i + 1;
        }
    }
}

fn ejecutar_elf_o_morir(linea: &[u8]) -> ! {
    let (cmd, args) = partir_palabra(linea);
    if cmd == b"echo" {
        sys_write(1, args);
        sys_write(1, b"\n");
        sys_exit(0);
    }
    if cmd == b"cat" {
        let mut cbuf = [0u8; 128];
        if let Some(cstr) = a_cstr(args, &mut cbuf) {
            let fd = sys_open(cstr, O_RDONLY);
            if fd >= 0 {
                let mut b = [0u8; 256];
                loop {
                    let n = sys_read(fd as u64, &mut b);
                    if n <= 0 { break; }
                    sys_write(1, &b[..n as usize]);
                }
                sys_close(fd as u64);
                sys_exit(0);
            }
        }
        sys_write(1, b"cat: error al abrir\n");
        sys_exit(1);
    }

    let mut ruta_buf = [0u8; 128];
    let rl = if cmd.first() == Some(&b'/') {
        let n = cmd.len().min(126);
        ruta_buf[..n].copy_from_slice(&cmd[..n]);
        n
    } else {
        const P: &[u8] = b"/bin/";
        let n = cmd.len().min(120);
        ruta_buf[..P.len()].copy_from_slice(P);
        ruta_buf[P.len()..P.len() + n].copy_from_slice(&cmd[..n]);
        P.len() + n
    };
    ruta_buf[rl] = 0;
    let ruta_cstr = &ruta_buf[..=rl];

    let mut arg_storage = [0u8; 128];
    let mut arg_ptrs: [*const u8; 10] = [core::ptr::null(); 10];
    arg_ptrs[0] = ruta_cstr.as_ptr();
    let mut argc = 1usize;
    let mut off = 0usize;
    let mut rem = recortar(args);
    while !rem.is_empty() && argc < 9 {
        let (pal, sig) = partir_palabra(rem);
        if off + pal.len() + 1 > arg_storage.len() { break; }
        arg_storage[off..off + pal.len()].copy_from_slice(pal);
        arg_storage[off + pal.len()] = 0;
        arg_ptrs[argc] = unsafe { arg_storage.as_ptr().add(off) };
        off += pal.len() + 1;
        argc += 1;
        rem = sig;
    }
    arg_ptrs[argc] = core::ptr::null();

    sys_execve(ruta_cstr, &arg_ptrs[..=argc]);
    sys_write(1, b"Comando no encontrado: ");
    sys_write(1, cmd);
    sys_write(1, b"\n");
    sys_exit(127);
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let fb_fd = sys_open(b"/dev/fb0\0", O_RDWR);
    if fb_fd < 0 {
        sys_write(1, b"desktop: /dev/fb0 no disponible\n");
        sys_exit(42);
    }
    let mut vinfo = [0u32; 40];
    sys_ioctl(fb_fd as u64, FBIOGET_VSCREENINFO, vinfo.as_mut_ptr() as u64);

    let fb_bytes = W * H * 4;
    let fb_ptr = sys_mmap(fb_bytes, MAP_SHARED, fb_fd as i64);
    let back_ptr = sys_mmap(fb_bytes, MAP_PRIVATE | MAP_ANONYMOUS, -1);
    let wall_ptr = sys_mmap(fb_bytes, MAP_PRIVATE | MAP_ANONYMOUS, -1);
    let blur_ptr = sys_mmap(fb_bytes, MAP_PRIVATE | MAP_ANONYMOUS, -1);
    let paint_ptr = sys_mmap(PAINT_W * PAINT_H * 4, MAP_PRIVATE | MAP_ANONYMOUS, -1);
    let estado_ptr = sys_mmap(
        core::mem::size_of::<EstadoOS>() + 4096,
        MAP_PRIVATE | MAP_ANONYMOUS,
        -1,
    ) as *mut EstadoOS;

    if fb_ptr.is_null()
        || back_ptr.is_null()
        || wall_ptr.is_null()
        || blur_ptr.is_null()
        || estado_ptr.is_null()
    {
        sys_exit(1);
    }

    let fb_slice = unsafe { core::slice::from_raw_parts_mut(fb_ptr, W * H) };
    let back_slice = unsafe { core::slice::from_raw_parts_mut(back_ptr, W * H) };
    let wall_slice = unsafe { core::slice::from_raw_parts_mut(wall_ptr, W * H) };
    let blur_slice = unsafe { core::slice::from_raw_parts_mut(blur_ptr, W * H) };

    let estado = unsafe { &mut *estado_ptr };
    estado.inicializar(paint_ptr);

    generar_wallpaper(estado.tema.wallpaper, wall_slice, leer_foto_wallpaper, back_slice);
    generar_blur(wall_slice, blur_slice, back_slice);

    let ev_fd = sys_open(b"/dev/input/events\0", O_RDONLY);

    let mut escena_sucia = true;
    let mut cursor_x = estado.mx;
    let mut cursor_y = estado.my;
    let mut ultimo_seg = sys_uptime_sec();
    let mut ev_buf = [0u8; 32 * 16];

    loop {
        if ev_fd >= 0 {
            let n = sys_read(ev_fd as u64, &mut ev_buf);
            if n > 0 {
                let cant = (n as usize) / core::mem::size_of::<EventoEntrada>();
                for i in 0..cant {
                    let ev = unsafe {
                        core::ptr::read_unaligned(
                            ev_buf.as_ptr().add(i * core::mem::size_of::<EventoEntrada>())
                                as *const EventoEntrada,
                        )
                    };
                    if estado.procesar_evento(ev) {
                        escena_sucia = true;
                    }
                }
            }
        }

        if let Some(nuevo_w) = estado.cambiar_wallpaper_a.take() {
            estado.tema.wallpaper = nuevo_w % 4;
            estado.guardar_tema_disco();
            generar_wallpaper(estado.tema.wallpaper, wall_slice, leer_foto_wallpaper, back_slice);
            generar_blur(wall_slice, blur_slice, back_slice);
            escena_sucia = true;
        }

        if estado.salir_a_consola {
            sys_ioctl(fb_fd as u64, FBIO_DESACTIVAR, 0);
            sys_close(fb_fd as u64);
            if ev_fd >= 0 { sys_close(ev_fd as u64); }
            sys_exit(42);
        }

        let ahora = sys_uptime_sec();
        if ahora != ultimo_seg {
            ultimo_seg = ahora;
            if estado.exp_msg_guardado > 0 {
                estado.exp_msg_guardado -= 1;
            }
            estado.monitor_refrescar();
            escena_sucia = true;
        }

        if escena_sucia {
            let mut lienzo = Lienzo {
                px: back_slice,
                wall: wall_slice,
                blur: blur_slice,
            };
            estado.dibujar(&mut lienzo);
            fb_slice.copy_from_slice(lienzo.px);
            dibujar_cursor_en(fb_slice, estado.mx, estado.my);
            cursor_x = estado.mx;
            cursor_y = estado.my;
            escena_sucia = false;
        } else if estado.mx != cursor_x || estado.my != cursor_y {
            // 0-lag: solo restauramos el parche 16x16 anterior y pintamos el cursor nuevo!
            restaurar_cursor_fb(fb_slice, back_slice, cursor_x, cursor_y);
            dibujar_cursor_en(fb_slice, estado.mx, estado.my);
            cursor_x = estado.mx;
            cursor_y = estado.my;
        }

        if estado.doom.destello > 0 {
            estado.doom.destello -= 1;
            escena_sucia = true;
        }

        if ev_fd >= 0 {
            sys_poll_entrada(ev_fd as i32, if estado.doom.destello > 0 { 24 } else { 60 });
        } else {
            sys_sleep_ms(16);
        }
    }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    sys_exit(1)
}
