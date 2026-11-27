// Driver de pantalla en modo texto VGA (80 x 25 caracteres).
use core::fmt;
use spin::Mutex;

const ANCHO: usize = 80;
const ALTO: usize = 25;
const VGA: *mut u16 = 0xb8000 as *mut u16;

#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum Color {
    Negro = 0,
    Azul = 1,
    Verde = 2,
    Cian = 3,
    Rojo = 4,
    Magenta = 5,
    Cafe = 6,
    GrisClaro = 7,
    GrisOscuro = 8,
    AzulClaro = 9,
    VerdeClaro = 10,
    CianClaro = 11,
    RojoClaro = 12,
    Rosa = 13,
    Amarillo = 14,
    Blanco = 15,
}

pub struct Escritor {
    columna: usize,
    color: u8,
}

impl Escritor {
    pub const fn new(texto: Color, fondo: Color) -> Self {
        Escritor {
            columna: 0,
            color: (fondo as u8) << 4 | (texto as u8),
        }
    }

    pub fn set_color(&mut self, texto: Color, fondo: Color) {
        self.color = (fondo as u8) << 4 | (texto as u8);
    }

    fn poner(&self, fila: usize, col: usize, byte: u8) {
        let celda = (self.color as u16) << 8 | byte as u16;
        unsafe { VGA.add(fila * ANCHO + col).write_volatile(celda) }
    }

    fn leer(fila: usize, col: usize) -> u16 {
        unsafe { VGA.add(fila * ANCHO + col).read_volatile() }
    }

    pub fn escribir_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.nueva_linea(),
            0x08 => self.borrar_ultimo(),
            byte => {
                if self.columna >= ANCHO {
                    self.nueva_linea();
                }
                self.poner(ALTO - 1, self.columna, byte);
                self.columna += 1;
            }
        }
    }

    pub fn escribir_texto(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' | 0x08 => self.escribir_byte(byte),
                _ => self.escribir_byte(0xfe),
            }
        }
    }

    fn borrar_ultimo(&mut self) {
        if self.columna > 0 {
            self.columna -= 1;
            self.poner(ALTO - 1, self.columna, b' ');
        }
    }

    fn nueva_linea(&mut self) {
        for fila in 1..ALTO {
            for col in 0..ANCHO {
                let celda = Self::leer(fila, col);
                unsafe { VGA.add((fila - 1) * ANCHO + col).write_volatile(celda) }
            }
        }
        self.limpiar_fila(ALTO - 1);
        self.columna = 0;
    }

    fn limpiar_fila(&self, fila: usize) {
        for col in 0..ANCHO {
            self.poner(fila, col, b' ');
        }
    }

    pub fn limpiar_pantalla(&mut self) {
        for fila in 0..ALTO {
            self.limpiar_fila(fila);
        }
        self.columna = 0;
    }
}

impl fmt::Write for Escritor {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.escribir_texto(s);
        Ok(())
    }
}

pub static PANTALLA: Mutex<Escritor> = Mutex::new(Escritor::new(Color::Blanco, Color::Negro));

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

pub fn limpiar() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        PANTALLA.lock().limpiar_pantalla();
    });
}

pub fn color(texto: Color, fondo: Color) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        PANTALLA.lock().set_color(texto, fondo);
    });
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    x86_64::instructions::interrupts::without_interrupts(|| {
        PANTALLA.lock().write_fmt(args).unwrap();
    });
}