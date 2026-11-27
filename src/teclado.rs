// Cola de entrada del teclado para stdin (fd 0) en modo canonico (linea con eco y Backspace).
use crate::print;
use spin::Mutex;
use x86_64::instructions::interrupts;

struct ColaTeclado {
    buf: [u8; 256],
    lectura: usize,
    escritura: usize,
}

impl ColaTeclado {
    const fn nueva() -> Self {
        ColaTeclado {
            buf: [0; 256],
            lectura: 0,
            escritura: 0,
        }
    }

    fn empujar(&mut self, byte: u8) {
        let siguiente = (self.escritura + 1) % self.buf.len();
        if siguiente != self.lectura {
            self.buf[self.escritura] = byte;
            self.escritura = siguiente;
        }
    }

    fn sacar(&mut self) -> Option<u8> {
        if self.lectura == self.escritura {
            return None;
        }
        let b = self.buf[self.lectura];
        self.lectura = (self.lectura + 1) % self.buf.len();
        Some(b)
    }
}

static COLA: Mutex<ColaTeclado> = Mutex::new(ColaTeclado::nueva());

/// Llamado desde la interrupcion de teclado (IRQ 1).
pub fn empujar_byte(byte: u8) {
    COLA.lock().empujar(byte);
}

/// Espera una tecla durmiendo con `hlt`. Entra y sale con interrupciones deshabilitadas.
fn leer_byte_bloqueante() -> u8 {
    loop {
        if let Some(b) = interrupts::without_interrupts(|| COLA.lock().sacar()) {
            return b;
        }
        interrupts::enable_and_hlt();
        interrupts::disable();
    }
}

/// Lee una linea del teclado (con eco y soporte de Backspace sin borrar el prompt).
pub fn leer_linea(dst: &mut [u8]) -> usize {
    if dst.is_empty() {
        return 0;
    }
    let mut pos = 0;
    loop {
        let b = leer_byte_bloqueante();
        match b {
            b'\n' | b'\r' => {
                print!("\n");
                if pos < dst.len() {
                    dst[pos] = b'\n';
                    pos += 1;
                }
                return pos;
            }
            0x08 => {
                if pos > 0 {
                    pos -= 1;
                    print!("\x08");
                }
            }
            0x20..=0x7e => {
                if pos + 1 < dst.len() {
                    dst[pos] = b;
                    pos += 1;
                    print!("{}", b as char);
                }
            }
            _ => {}
        }
    }
}
