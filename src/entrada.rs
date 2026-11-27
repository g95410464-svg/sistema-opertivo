// Cola de eventos crudos de teclado y raton para `/dev/input/events` (no bloqueante).
use spin::Mutex;
use x86_64::instructions::interrupts;

pub const TIPO_TECLA_ABAJO: u16 = 1;
pub const TIPO_TECLA_ARRIBA: u16 = 2;
pub const TIPO_RATON: u16 = 3;

pub const TECLA_ARRIBA: u16 = 0x100;
pub const TECLA_ABAJO: u16 = 0x101;
pub const TECLA_IZQ: u16 = 0x102;
pub const TECLA_DER: u16 = 0x103;
pub const TECLA_SUPR: u16 = 0x104;
pub const TECLA_INICIO: u16 = 0x105;
pub const TECLA_FIN: u16 = 0x106;
pub const TECLA_PGUP: u16 = 0x107;
pub const TECLA_PGDN: u16 = 0x108;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct EventoEntrada {
    pub tipo: u16,
    pub codigo: u16,
    pub x: i32,
    pub y: i32,
    pub botones: u32,
}

const CAP: usize = 128;

struct ColaEventos {
    buf: [EventoEntrada; CAP],
    lectura: usize,
    escritura: usize,
    ultimo_x: i32,
    ultimo_y: i32,
    ultimos_botones: u32,
}

impl ColaEventos {
    const fn nueva() -> Self {
        const VACIO: EventoEntrada = EventoEntrada {
            tipo: 0,
            codigo: 0,
            x: 0,
            y: 0,
            botones: 0,
        };
        ColaEventos {
            buf: [VACIO; CAP],
            lectura: 0,
            escritura: 0,
            ultimo_x: 640,
            ultimo_y: 360,
            ultimos_botones: 0,
        }
    }

    fn empujar(&mut self, ev: EventoEntrada) {
        // Si el evento anterior tambien era movimiento puro de raton (sin rueda y mismos botones), lo fusionamos.
        if ev.tipo == TIPO_RATON && ev.codigo == 0 && self.lectura != self.escritura {
            let prev = (self.escritura + CAP - 1) % CAP;
            if self.buf[prev].tipo == TIPO_RATON
                && self.buf[prev].codigo == 0
                && self.buf[prev].botones == ev.botones
            {
                self.buf[prev] = ev;
                return;
            }
        }
        let sig = (self.escritura + 1) % CAP;
        if sig != self.lectura {
            self.buf[self.escritura] = ev;
            self.escritura = sig;
        }
    }

    fn sacar(&mut self) -> Option<EventoEntrada> {
        if self.lectura == self.escritura {
            return None;
        }
        let ev = self.buf[self.lectura];
        self.lectura = (self.lectura + 1) % CAP;
        Some(ev)
    }
}

static COLA: Mutex<ColaEventos> = Mutex::new(ColaEventos::nueva());

pub fn empujar_tecla(presionada: bool, codigo: u16) {
    let mut c = COLA.lock();
    let ev = EventoEntrada {
        tipo: if presionada {
            TIPO_TECLA_ABAJO
        } else {
            TIPO_TECLA_ARRIBA
        },
        codigo,
        x: c.ultimo_x,
        y: c.ultimo_y,
        botones: c.ultimos_botones,
    };
    c.empujar(ev);
}

pub fn empujar_raton(x: i32, y: i32, botones: u32, rueda: i8) {
    let mut c = COLA.lock();
    c.ultimo_x = x;
    c.ultimo_y = y;
    c.ultimos_botones = botones;
    c.empujar(EventoEntrada {
        tipo: TIPO_RATON,
        codigo: (rueda as i16) as u16,
        x,
        y,
        botones,
    });
}

/// Indica si hay eventos de teclado o raton pendientes en la cola (sin bloquear).
pub fn hay_eventos() -> bool {
    interrupts::without_interrupts(|| {
        let c = COLA.lock();
        c.lectura != c.escritura
    })
}

/// Lee eventos disponibles sin bloquear. Devuelve bytes escritos (multiplo de 16).
pub fn leer_eventos(dst: &mut [u8]) -> usize {
    let tam = core::mem::size_of::<EventoEntrada>();
    let mut escrito = 0usize;
    interrupts::without_interrupts(|| {
        let mut c = COLA.lock();
        while escrito + tam <= dst.len() {
            let Some(ev) = c.sacar() else {
                break;
            };
            let bytes = unsafe {
                core::slice::from_raw_parts(&ev as *const EventoEntrada as *const u8, tam)
            };
            dst[escrito..escrito + tam].copy_from_slice(bytes);
            escrito += tam;
        }
    });
    escrito
}
