// Shell interactiva en modo usuario (Ring 3) — binario ELF64 real de Linux x86_64.
// Soporta cd/pwd, pipes (`cmd1 | cmd2`), redireccion (`cmd > archivo`), mmap, fork/execve/wait4.
// Compilar con:
//   rustc +nightly --target x86_64-unknown-none -O -C panic=abort -C strip=symbols -C relocation-model=static -C link-arg=--image-base=0x400000 programas/sh.rs -o programas/sh.elf
#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

// Numeros oficiales de syscalls de Linux x86_64
const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_CLOSE: u64 = 3;
const SYS_MMAP: u64 = 9;
const SYS_BRK: u64 = 12;
const SYS_PIPE: u64 = 22;
const SYS_DUP2: u64 = 33;
const SYS_GETPID: u64 = 39;
const SYS_FORK: u64 = 57;
const SYS_EXECVE: u64 = 59;
const SYS_EXIT: u64 = 60;
const SYS_WAIT4: u64 = 61;
const SYS_UNAME: u64 = 63;
const SYS_GETCWD: u64 = 79;
const SYS_CHDIR: u64 = 80;
const SYS_GETDENTS64: u64 = 217;
const SYS_OPENAT: u64 = 257;

const AT_FDCWD: i64 = -100;
const O_RDONLY: u64 = 0;
const O_WRONLY: u64 = 1;
const O_CREAT: u64 = 0x40;
const O_TRUNC: u64 = 0x200;

const PROT_READ: u64 = 1;
const PROT_WRITE: u64 = 2;
const MAP_PRIVATE: u64 = 0x02;
const MAP_ANONYMOUS: u64 = 0x20;

const DT_DIR: u8 = 4;

#[repr(C)]
struct UtsName {
    sysname: [u8; 65],
    nodename: [u8; 65],
    release: [u8; 65],
    version: [u8; 65],
    machine: [u8; 65],
    domainname: [u8; 65],
}

#[inline(always)]
unsafe fn syscall0(num: u64) -> isize {
    let ret: isize;
    asm!("syscall", in("rax") num, lateout("rax") ret, out("rcx") _, out("r11") _);
    ret
}

#[inline(always)]
unsafe fn syscall1(num: u64, a1: u64) -> isize {
    let ret: isize;
    asm!("syscall", in("rax") num, in("rdi") a1, lateout("rax") ret, out("rcx") _, out("r11") _);
    ret
}

#[inline(always)]
unsafe fn syscall2(num: u64, a1: u64, a2: u64) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        in("rax") num, in("rdi") a1, in("rsi") a2,
        lateout("rax") ret, out("rcx") _, out("r11") _
    );
    ret
}

#[inline(always)]
unsafe fn syscall3(num: u64, a1: u64, a2: u64, a3: u64) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        in("rax") num, in("rdi") a1, in("rsi") a2, in("rdx") a3,
        lateout("rax") ret, out("rcx") _, out("r11") _
    );
    ret
}

#[inline(always)]
unsafe fn syscall4(num: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        in("rax") num, in("rdi") a1, in("rsi") a2, in("rdx") a3, in("r10") a4,
        lateout("rax") ret, out("rcx") _, out("r11") _
    );
    ret
}

#[inline(always)]
unsafe fn syscall6(num: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        in("rax") num,
        in("rdi") a1, in("rsi") a2, in("rdx") a3,
        in("r10") a4, in("r8") a5, in("r9") a6,
        lateout("rax") ret, out("rcx") _, out("r11") _
    );
    ret
}

fn sys_read(fd: u64, buf: &mut [u8]) -> isize {
    unsafe { syscall3(SYS_READ, fd, buf.as_mut_ptr() as u64, buf.len() as u64) }
}

fn sys_write(fd: u64, buf: &[u8]) -> isize {
    unsafe { syscall3(SYS_WRITE, fd, buf.as_ptr() as u64, buf.len() as u64) }
}

fn sys_openat(ruta_cstr: &[u8], flags: u64) -> isize {
    unsafe { syscall4(SYS_OPENAT, AT_FDCWD as u64, ruta_cstr.as_ptr() as u64, flags, 0o644) }
}

fn sys_close(fd: u64) -> isize {
    unsafe { syscall1(SYS_CLOSE, fd) }
}

fn sys_pipe(fds: &mut [i32; 2]) -> isize {
    unsafe { syscall1(SYS_PIPE, fds.as_mut_ptr() as u64) }
}

fn sys_dup2(viejo: u64, nuevo: u64) -> isize {
    unsafe { syscall2(SYS_DUP2, viejo, nuevo) }
}

fn sys_brk(addr: u64) -> u64 {
    unsafe { syscall1(SYS_BRK, addr) as u64 }
}

fn sys_mmap_anon(tam: u64) -> isize {
    unsafe {
        syscall6(
            SYS_MMAP,
            0,
            tam,
            PROT_READ | PROT_WRITE,
            MAP_PRIVATE | MAP_ANONYMOUS,
            u64::MAX,
            0,
        )
    }
}

fn sys_getcwd(buf: &mut [u8]) -> isize {
    unsafe { syscall2(SYS_GETCWD, buf.as_mut_ptr() as u64, buf.len() as u64) }
}

fn sys_chdir(ruta_cstr: &[u8]) -> isize {
    unsafe { syscall1(SYS_CHDIR, ruta_cstr.as_ptr() as u64) }
}

fn sys_getpid() -> isize {
    unsafe { syscall0(SYS_GETPID) }
}

fn sys_fork() -> isize {
    unsafe { syscall0(SYS_FORK) }
}

fn sys_execve(ruta_cstr: &[u8], argv: &[*const u8]) -> isize {
    let envp: [*const u8; 1] = [core::ptr::null()];
    unsafe {
        syscall3(
            SYS_EXECVE,
            ruta_cstr.as_ptr() as u64,
            argv.as_ptr() as u64,
            envp.as_ptr() as u64,
        )
    }
}

fn sys_wait4(pid: i64, status: &mut i32) -> isize {
    unsafe { syscall4(SYS_WAIT4, pid as u64, status as *mut i32 as u64, 0, 0) }
}

fn sys_uname(buf: &mut UtsName) -> isize {
    unsafe { syscall1(SYS_UNAME, buf as *mut UtsName as u64) }
}

fn sys_getdents64(fd: u64, buf: &mut [u8]) -> isize {
    unsafe { syscall3(SYS_GETDENTS64, fd, buf.as_mut_ptr() as u64, buf.len() as u64) }
}

fn sys_exit(code: u64) -> ! {
    unsafe {
        asm!("syscall", in("rax") SYS_EXIT, in("rdi") code, options(noreturn));
    }
}

fn escribir(s: &[u8]) {
    sys_write(1, s);
}

fn escribir_cstr(buf: &[u8]) {
    let mut len = 0;
    while len < buf.len() && buf[len] != 0 {
        len += 1;
    }
    escribir(&buf[..len]);
}

fn escribir_num(mut n: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    if n == 0 {
        escribir(b"0");
        return;
    }
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    escribir(&buf[i..]);
}

fn escribir_hex(mut n: u64) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut buf = [0u8; 16];
    let mut i = buf.len();
    escribir(b"0x");
    if n == 0 {
        escribir(b"0");
        return;
    }
    while n > 0 {
        i -= 1;
        buf[i] = HEX[(n & 0xf) as usize];
        n >>= 4;
    }
    escribir(&buf[i..]);
}

fn a_cstr<'a>(s: &[u8], dst: &'a mut [u8; 128]) -> Option<&'a [u8]> {
    if s.is_empty() || s.len() >= dst.len() {
        return None;
    }
    dst[..s.len()].copy_from_slice(s);
    dst[s.len()] = 0;
    Some(&dst[..=s.len()])
}

fn recortar(mut s: &[u8]) -> &[u8] {
    while let Some(&b) = s.first() {
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            s = &s[1..];
        } else {
            break;
        }
    }
    while let Some(&b) = s.last() {
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            s = &s[..s.len() - 1];
        } else {
            break;
        }
    }
    s
}

fn partir_primera_palabra(s: &[u8]) -> (&[u8], &[u8]) {
    let s = recortar(s);
    for (i, &b) in s.iter().enumerate() {
        if b == b' ' || b == b'\t' {
            return (&s[..i], recortar(&s[i + 1..]));
        }
    }
    (s, b"")
}

fn dividir_en_byte(s: &[u8], sep: u8) -> Option<(&[u8], &[u8])> {
    for (i, &b) in s.iter().enumerate() {
        if b == sep {
            return Some((recortar(&s[..i]), recortar(&s[i + 1..])));
        }
    }
    None
}

fn cmd_cat(ruta: &[u8]) {
    let mut cbuf = [0u8; 128];
    let Some(cstr) = a_cstr(ruta, &mut cbuf) else {
        escribir(b"Uso: cat <ruta>\n");
        return;
    };
    let fd = sys_openat(cstr, O_RDONLY);
    if fd < 0 {
        escribir(b"cat: no se pudo abrir ");
        escribir(ruta);
        escribir(b"\n");
        return;
    }
    let mut buf = [0u8; 256];
    let mut ultimo = b'\n';
    loop {
        let n = sys_read(fd as u64, &mut buf);
        if n <= 0 {
            break;
        }
        let slice = &buf[..n as usize];
        escribir(slice);
        ultimo = slice[slice.len() - 1];
    }
    if ultimo != b'\n' {
        escribir(b"\n");
    }
    sys_close(fd as u64);
}

fn cmd_ls(ruta: &[u8]) {
    let ruta = if ruta.is_empty() { b"." } else { ruta };
    let mut cbuf = [0u8; 128];
    let Some(cstr) = a_cstr(ruta, &mut cbuf) else {
        escribir(b"ls: ruta invalida\n");
        return;
    };
    let fd = sys_openat(cstr, O_RDONLY);
    if fd < 0 {
        escribir(b"ls: no existe ");
        escribir(ruta);
        escribir(b"\n");
        return;
    }
    let mut buf = [0u8; 512];
    loop {
        let n = sys_getdents64(fd as u64, &mut buf);
        if n <= 0 {
            break;
        }
        let mut pos = 0usize;
        while pos + 19 < n as usize {
            let reclen = u16::from_le_bytes([buf[pos + 16], buf[pos + 17]]) as usize;
            if reclen < 19 || pos + reclen > n as usize {
                break;
            }
            let dtype = buf[pos + 18];
            let nombre_slice = &buf[pos + 19..pos + reclen];
            if dtype == DT_DIR {
                escribir(b"\x1b[96m");
                escribir_cstr(nombre_slice);
                escribir(b"/\x1b[0m  ");
            } else {
                escribir_cstr(nombre_slice);
                escribir(b"  ");
            }
            pos += reclen;
        }
    }
    escribir(b"\n");
    sys_close(fd as u64);
}

fn cmd_write(args: &[u8]) {
    let (ruta, contenido) = partir_primera_palabra(args);
    let mut cbuf = [0u8; 128];
    let Some(cstr) = a_cstr(ruta, &mut cbuf) else {
        escribir(b"Uso: write <archivo> <texto>\n");
        return;
    };
    let fd = sys_openat(cstr, O_WRONLY | O_CREAT | O_TRUNC);
    if fd < 0 {
        escribir(b"write: no se pudo crear ");
        escribir(ruta);
        escribir(b"\n");
        return;
    }
    sys_write(fd as u64, contenido);
    sys_write(fd as u64, b"\n");
    sys_close(fd as u64);
    escribir(b"Guardado en ");
    escribir(ruta);
    escribir(b"\n");
}

fn cmd_uname() {
    let mut u = UtsName {
        sysname: [0; 65],
        nodename: [0; 65],
        release: [0; 65],
        version: [0; 65],
        machine: [0; 65],
        domainname: [0; 65],
    };
    if sys_uname(&mut u) == 0 {
        escribir_cstr(&u.sysname);
        escribir(b" ");
        escribir_cstr(&u.nodename);
        escribir(b" ");
        escribir_cstr(&u.release);
        escribir(b" ");
        escribir_cstr(&u.machine);
        escribir(b"\n");
    }
}

fn cmd_brk() {
    let actual = sys_brk(0);
    let nuevo = sys_brk(actual + 4096);
    escribir(b"brk: ");
    escribir_hex(actual);
    escribir(b" -> ");
    escribir_hex(nuevo);
    escribir(b"\n");
}

fn cmd_mmap() {
    let ptr = sys_mmap_anon(4096);
    if ptr < 0 {
        escribir(b"mmap fallo\n");
        return;
    }
    unsafe {
        let p = ptr as *mut u64;
        p.write_volatile(0xDEAD_C0DE);
        let v = p.read_volatile();
        escribir(b"mmap(4096) = ");
        escribir_hex(ptr as u64);
        escribir(b" | escrito y leido = ");
        escribir_hex(v);
        escribir(b"\n");
    }
}

fn cmd_cd(ruta: &[u8]) {
    let ruta = if ruta.is_empty() { b"/" } else { ruta };
    let mut cbuf = [0u8; 128];
    let Some(cstr) = a_cstr(ruta, &mut cbuf) else {
        return;
    };
    if sys_chdir(cstr) < 0 {
        escribir(b"cd: no es un directorio valido: ");
        escribir(ruta);
        escribir(b"\n");
    }
}

fn cmd_pwd() {
    let mut buf = [0u8; 128];
    if sys_getcwd(&mut buf) > 0 {
        escribir_cstr(&buf);
        escribir(b"\n");
    }
}

/// Ejecuta un comando simple en el proceso actual (ya dentro del hijo cuando hay pipe/redireccion).
fn ejecutar_comando_en_proceso_actual(linea: &[u8]) -> ! {
    let (cmd, args) = partir_primera_palabra(linea);
    match cmd {
        b"echo" => {
            escribir(args);
            escribir(b"\n");
            sys_exit(0);
        }
        b"cat" => {
            cmd_cat(args);
            sys_exit(0);
        }
        b"ls" => {
            cmd_ls(args);
            sys_exit(0);
        }
        b"ps" => {
            cmd_cat(b"/proc/ps");
            sys_exit(0);
        }
        b"uname" => {
            cmd_uname();
            sys_exit(0);
        }
        b"pwd" => {
            cmd_pwd();
            sys_exit(0);
        }
        _ => {}
    }

    // Buscamos binario ELF en ruta directa o en `/bin/<cmd>`.
    let mut ruta_buf = [0u8; 128];
    let ruta_len = if cmd.first() == Some(&b'/') {
        if cmd.len() >= ruta_buf.len() {
            sys_exit(127);
        }
        ruta_buf[..cmd.len()].copy_from_slice(cmd);
        cmd.len()
    } else {
        const PREFIJO: &[u8] = b"/bin/";
        if PREFIJO.len() + cmd.len() >= ruta_buf.len() {
            sys_exit(127);
        }
        ruta_buf[..PREFIJO.len()].copy_from_slice(PREFIJO);
        ruta_buf[PREFIJO.len()..PREFIJO.len() + cmd.len()].copy_from_slice(cmd);
        PREFIJO.len() + cmd.len()
    };
    ruta_buf[ruta_len] = 0;
    let ruta_cstr = &ruta_buf[..=ruta_len];

    let mut arg_storage = [0u8; 128];
    let mut arg_ptrs: [*const u8; 10] = [core::ptr::null(); 10];
    arg_ptrs[0] = ruta_cstr.as_ptr();
    let mut argc = 1usize;
    let mut off = 0usize;
    let mut rem = recortar(args);
    while !rem.is_empty() && argc < 9 {
        let (pal, sig) = partir_primera_palabra(rem);
        if off + pal.len() + 1 > arg_storage.len() {
            break;
        }
        arg_storage[off..off + pal.len()].copy_from_slice(pal);
        arg_storage[off + pal.len()] = 0;
        arg_ptrs[argc] = unsafe { arg_storage.as_ptr().add(off) };
        off += pal.len() + 1;
        argc += 1;
        rem = sig;
    }
    arg_ptrs[argc] = core::ptr::null();

    sys_execve(ruta_cstr, &arg_ptrs[..=argc]);
    escribir(b"Comando desconocido: ");
    escribir(cmd);
    escribir(b" (escribe 'help')\n");
    sys_exit(127);
}

/// Ejecuta `izq | der` (y opcionalmente `> archivo` en `der`) usando pipe() + dup2() + fork().
fn ejecutar_pipe(izq: &[u8], der: &[u8]) {
    let (der_cmd, archivo_out) = match dividir_en_byte(der, b'>') {
        Some((c, f)) => (c, Some(f)),
        None => (der, None),
    };

    let mut fds = [0i32; 2];
    if sys_pipe(&mut fds) < 0 {
        escribir(b"sh: error en pipe()\n");
        return;
    }

    let p1 = sys_fork();
    if p1 == 0 {
        sys_dup2(fds[1] as u64, 1);
        sys_close(fds[0] as u64);
        sys_close(fds[1] as u64);
        ejecutar_comando_en_proceso_actual(izq);
    }

    let p2 = sys_fork();
    if p2 == 0 {
        sys_dup2(fds[0] as u64, 0);
        sys_close(fds[0] as u64);
        sys_close(fds[1] as u64);
        if let Some(ruta) = archivo_out {
            let mut cbuf = [0u8; 128];
            if let Some(cstr) = a_cstr(ruta, &mut cbuf) {
                let fd = sys_openat(cstr, O_WRONLY | O_CREAT | O_TRUNC);
                if fd >= 0 {
                    sys_dup2(fd as u64, 1);
                    sys_close(fd as u64);
                }
            }
        }
        ejecutar_comando_en_proceso_actual(der_cmd);
    }

    sys_close(fds[0] as u64);
    sys_close(fds[1] as u64);

    let mut status = 0i32;
    if p1 > 0 {
        sys_wait4(p1 as i64, &mut status);
    }
    if p2 > 0 {
        sys_wait4(p2 as i64, &mut status);
    }
}

/// Ejecuta `cmd > archivo` redirigiendo stdout (FD 1) con dup2().
fn ejecutar_redireccion(cmd_linea: &[u8], ruta: &[u8]) {
    let mut cbuf = [0u8; 128];
    let Some(cstr) = a_cstr(ruta, &mut cbuf) else {
        escribir(b"Redireccion invalida\n");
        return;
    };
    let hijo = sys_fork();
    if hijo == 0 {
        let fd = sys_openat(cstr, O_WRONLY | O_CREAT | O_TRUNC);
        if fd < 0 {
            escribir(b"No se pudo abrir archivo destino\n");
            sys_exit(1);
        }
        sys_dup2(fd as u64, 1);
        sys_close(fd as u64);
        ejecutar_comando_en_proceso_actual(cmd_linea);
    } else if hijo > 0 {
        let mut status = 0i32;
        sys_wait4(hijo as i64, &mut status);
    }
}

fn ejecutar_simple(linea: &[u8]) {
    let (cmd, args) = partir_primera_palabra(linea);
    if cmd.is_empty() {
        return;
    }
    match cmd {
        b"help" => {
            escribir(b"Comandos internos:\n");
            escribir(b"  cd <dir>, pwd, ls [dir], cat <ruta>, echo <txt>, write <ruta> <txt>\n");
            escribir(b"  ps, uname, pid, brk, mmap, clear, exit\n");
            escribir(b"Programas ELF en /bin (fork + execve + wait4):\n");
            escribir(b"  hola [nombre], forktest, upper\n");
            escribir(b"Pipes y redireccion (pipe + dup2):\n");
            escribir(b"  hola Guillermo | upper\n");
            escribir(b"  cat /etc/hostname | upper > /tmp/host.txt\n");
        }
        b"cd" => cmd_cd(args),
        b"pwd" => cmd_pwd(),
        b"ls" => cmd_ls(args),
        b"cat" => cmd_cat(args),
        b"echo" => {
            escribir(args);
            escribir(b"\n");
        }
        b"write" => cmd_write(args),
        b"ps" => cmd_cat(b"/proc/ps"),
        b"uname" => cmd_uname(),
        b"pid" => {
            escribir(b"PID = ");
            escribir_num(sys_getpid() as u64);
            escribir(b"\n");
        }
        b"brk" => cmd_brk(),
        b"mmap" => cmd_mmap(),
        b"clear" => escribir(b"\x1b[2J"),
        b"exit" => {
            escribir(b"Hasta luego!\n");
            sys_exit(0);
        }
        _ => {
            let hijo = sys_fork();
            if hijo == 0 {
                ejecutar_comando_en_proceso_actual(linea);
            } else if hijo > 0 {
                let mut status = 0i32;
                sys_wait4(hijo as i64, &mut status);
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    cmd_cat(b"/etc/motd");

    let mut linea = [0u8; 160];
    let mut cwd = [0u8; 64];
    loop {
        escribir(b"\x1b[92mneo_ws:\x1b[96m");
        if sys_getcwd(&mut cwd) > 0 {
            escribir_cstr(&cwd);
        } else {
            escribir(b"/");
        }
        escribir(b"\x1b[92m>\x1b[0m ");

        let n = sys_read(0, &mut linea);
        if n <= 0 {
            continue;
        }
        let entrada = recortar(&linea[..n as usize]);
        if entrada.is_empty() {
            continue;
        }

        if let Some((izq, der)) = dividir_en_byte(entrada, b'|') {
            ejecutar_pipe(izq, der);
        } else if let Some((cmd, archivo)) = dividir_en_byte(entrada, b'>') {
            ejecutar_redireccion(cmd, archivo);
        } else {
            ejecutar_simple(entrada);
        }
    }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    sys_exit(1)
}
