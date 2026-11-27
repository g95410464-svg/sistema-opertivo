// Driver de raton PS/2 (IRQ 12): 200 Hz + rueda IntelliMouse (paquetes de 3 o 4 bytes).
use crate::{entrada, fb};
use spin::Mutex;
use x86_64::instructions::port::Port;

struct EstadoRaton {
    ciclo: u8,
    tiene_rueda: bool,
    paquete: [u8; 4],
    x: i32,
    y: i32,
    botones: u32,
}

static RATON: Mutex<EstadoRaton> = Mutex::new(EstadoRaton {
    ciclo: 0,
    tiene_rueda: false,
    paquete: [0; 4],
    x: 640,
    y: 360,
    botones: 0,
});

fn esperar_escritura() {
    let mut estado: Port<u8> = Port::new(0x64);
    for _ in 0..100_000 {
        if unsafe { estado.read() } & 0x02 == 0 {
            return;
        }
    }
}

fn esperar_lectura() -> bool {
    let mut estado: Port<u8> = Port::new(0x64);
    for _ in 0..100_000 {
        if unsafe { estado.read() } & 0x01 != 0 {
            return true;
        }
    }
    false
}

fn leer_byte_raton() -> u8 {
    let mut p_dat: Port<u8> = Port::new(0x60);
    if esperar_lectura() {
        unsafe { p_dat.read() }
    } else {
        0
    }
}

fn escribir_raton(cmd: u8) -> u8 {
    let mut p_cmd: Port<u8> = Port::new(0x64);
    let mut p_dat: Port<u8> = Port::new(0x60);
    esperar_escritura();
    unsafe { p_cmd.write(0xD4) };
    esperar_escritura();
    unsafe { p_dat.write(cmd) };
    leer_byte_raton()
}

fn configurar_tasa(hz: u8) {
    let _ = escribir_raton(0xF3);
    let _ = escribir_raton(hz);
}

/// Habilita el puerto auxiliar PS/2, activa IRQ 12, detecta rueda IntelliMouse y sube a 200 Hz.
pub fn init() {
    let mut p_cmd: Port<u8> = Port::new(0x64);
    let mut p_dat: Port<u8> = Port::new(0x60);

    // 1) Habilitar el dispositivo auxiliar (raton).
    esperar_escritura();
    unsafe { p_cmd.write(0xA8) };

    // 2) Leer byte de configuracion del controlador 8042 y activar IRQ 12 (bit 1) + reloj (limpiar bit 5).
    esperar_escritura();
    unsafe { p_cmd.write(0x20) };
    let mut config = if esperar_lectura() {
        unsafe { p_dat.read() }
    } else {
        0
    };
    config |= 0x02;
    config &= !0x20;

    esperar_escritura();
    unsafe { p_cmd.write(0x60) };
    esperar_escritura();
    unsafe { p_dat.write(config) };

    // 3) Valores por defecto + secuencia magica IntelliMouse (200, 100, 80) para activar rueda de scroll.
    let _ = escribir_raton(0xF6);
    configurar_tasa(200);
    configurar_tasa(100);
    configurar_tasa(80);
    let _ = escribir_raton(0xF2); // Get Device ID
    let id = leer_byte_raton();
    RATON.lock().tiene_rueda = id == 3;

    // 4) Subir tasa de muestreo al maximo PS/2 (200 Hz) para movimiento sin lag y activar streaming.
    configurar_tasa(200);
    let _ = escribir_raton(0xF4);
}

/// Procesa un byte recibido desde IRQ 12 (puerto 0x60).
pub fn procesar_byte(byte: u8) {
    let mut r = RATON.lock();
    let total = if r.tiene_rueda { 4 } else { 3 };
    let ciclo = r.ciclo as usize;

    if ciclo == 0 {
        // El bit 3 siempre debe ser 1 en el primer byte de un paquete PS/2 valido.
        if (byte & 0x08) == 0 {
            return;
        }
        r.paquete[0] = byte;
        r.ciclo = 1;
        return;
    }

    r.paquete[ciclo] = byte;
    r.ciclo += 1;
    if (r.ciclo as usize) < total {
        return;
    }
    r.ciclo = 0;

    let flags = r.paquete[0];
    if (flags & 0xC0) != 0 {
        return;
    }

    let dx = if (flags & 0x10) != 0 {
        r.paquete[1] as i32 - 256
    } else {
        r.paquete[1] as i32
    };
    let dy = if (flags & 0x20) != 0 {
        r.paquete[2] as i32 - 256
    } else {
        r.paquete[2] as i32
    };
    let rueda: i8 = if r.tiene_rueda {
        let z = (r.paquete[3] & 0x0F) as i8;
        if z >= 8 { z - 16 } else { z }
    } else {
        0
    };

    let max_x = (fb::ANCHO_FB as i32) - 1;
    let max_y = (fb::ALTO_FB as i32) - 1;
    r.x = (r.x + dx).clamp(0, max_x);
    r.y = (r.y - dy).clamp(0, max_y);
    r.botones = (flags & 0x07) as u32;

    let (x, y, botones) = (r.x, r.y, r.botones);
    drop(r);
    entrada::empujar_raton(x, y, botones, rueda);
}
