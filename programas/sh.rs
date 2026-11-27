// Shell interactiva en modo usuario (Ring 3) — binario ELF64 real de Linux x86_64.
// Se comunica con el kernel UNICAMENTE mediante syscalls oficiales de Linux x86_64.
// Compilar con:
//   rustc +nightly --target x86_64-unknown-none -O -C panic=abort -C relocation-model=static -C link-arg=--image-base=0x400000 programas/sh.rs -o programas/sh.elf
#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

// Numeros oficiales de syscalls de Linux x86_64
const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_CLOSE: u64 = 3;
const SYS_BRK: u64 = 12;
const SYS_GETPID: u64 = 39;
const SYS_EXIT: u64 = 60;
const SYS_UNAME: u64 = 63;
const SYS_GETDENTS64: u64 = 217;
const SYS_OPENAT: u64 = 257;

const AT_FDCWD: i64 = -100;
const O_RDONLY: u64 = 0;
const O_WRONLY: u64 = 1;
const O_CREAT: u64 = 0x40;
const O_TRUNC: u64 = 0x200;

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
unsafe fn syscall1(num: u64, a1: u64) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        in("rax") num,
        in("rdi") a1,
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
    );
    ret
}

#[inline(always)]
unsafe fn syscall3(num: u64, a1: u64, a2: u64, a3: u64) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        in("rax") num,
        in("rdi") a1,
        in("rsi") a2,
        in("rdx") a3,
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
    );
    ret
}

#[inline(always)]
unsafe fn syscall4(num: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> isize {
    let ret: isize;
    asm!(
        "syscall",
        in("rax") num,
        in("rdi") a1,
        in("rsi") a2,
        in("rdx") a3,
        in("r10") a4,
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
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

fn sys_brk(addr: u64) -> u64 {
    unsafe { syscall1(SYS_BRK, addr) as u64 }
}

fn sys_getpid() -> isize {
    unsafe { syscall1(SYS_GETPID, 0) }
}

fn sys_uname(buf: &mut UtsName) -> isize {
    unsafe { syscall1(SYS_UNAME, buf as *mut UtsName as u64) }
}

fn sys_getdents64(fd: u64, buf: &mut [u8]) -> isize {
    unsafe { syscall3(SYS_GETDENTS64, fd, buf.as_mut_ptr() as u64, buf.len() as u64) }
}

fn sys_exit(code: u64) -> ! {
    unsafe {
        asm!(
            "syscall",
            in("rax") SYS_EXIT,
            in("rdi") code,
            options(noreturn)
        );
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
    let ruta = if ruta.is_empty() { b"/" } else { ruta };
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
    escribir(b"brk anterior: ");
    escribir_hex(actual);
    escribir(b" -> nuevo: ");
    escribir_hex(nuevo);
    escribir(b"\n");
    if nuevo >= actual + 4096 {
        unsafe {
            let p = actual as *mut u64;
            p.write_volatile(0xCAFE_BABE);
            let v = p.read_volatile();
            escribir(b"Escrito y leido en heap de usuario: ");
            escribir_hex(v);
            escribir(b"\n");
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    cmd_cat(b"/etc/motd");

    let mut linea = [0u8; 128];
    loop {
        escribir(b"\x1b[92mneo_ws>\x1b[0m ");
        let n = sys_read(0, &mut linea);
        if n <= 0 {
            continue;
        }
        let (cmd, args) = partir_primera_palabra(&linea[..n as usize]);
        if cmd.is_empty() {
            continue;
        }
        match cmd {
            b"help" => {
                escribir(b"Comandos (corriendo en Ring 3 con syscalls de Linux):\n");
                escribir(b"  help                 - muestra esta ayuda\n");
                escribir(b"  ls [dir]             - lista archivos (openat + getdents64)\n");
                escribir(b"  cat <ruta>           - muestra un archivo (openat + read)\n");
                escribir(b"  write <ruta> <texto> - crea/escribe un archivo (openat + write)\n");
                escribir(b"  uname                - info del sistema (syscall uname)\n");
                escribir(b"  pid                  - PID actual (syscall getpid)\n");
                escribir(b"  brk                  - pide 4 KiB de RAM al kernel (syscall brk)\n");
                escribir(b"  clear                - limpia la pantalla (escape ANSI)\n");
                escribir(b"  exit                 - sale de la shell (syscall exit)\n");
            }
            b"ls" => cmd_ls(args),
            b"cat" => cmd_cat(args),
            b"write" => cmd_write(args),
            b"uname" => cmd_uname(),
            b"pid" => {
                escribir(b"PID = ");
                escribir_num(sys_getpid() as u64);
                escribir(b"\n");
            }
            b"brk" => cmd_brk(),
            b"clear" => escribir(b"\x1b[2J"),
            b"exit" => {
                escribir(b"Hasta luego!\n");
                sys_exit(0);
            }
            _ => {
                escribir(b"Comando desconocido: ");
                escribir(cmd);
                escribir(b" (escribe 'help')\n");
            }
        }
    }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    sys_exit(1)
}
