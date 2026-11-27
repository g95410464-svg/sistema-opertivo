// Manejo de excepciones del CPU e interrupciones de hardware (PIC, timer, teclado, raton PS/2).
use crate::{entrada, gdt, println, raton, teclado};
use core::sync::atomic::{AtomicU64, Ordering};
use pc_keyboard::{layouts, DecodedKey, HandleControl, KeyCode, KeyState, Keyboard, ScancodeSet1};
use pic8259::ChainedPics;
use spin::{Lazy, Mutex};
use x86_64::instructions::port::Port;
use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

// Los controladores PIC se remapean a partir de 32 para no chocar con las excepciones del CPU (0..31).
pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

#[derive(Clone, Copy)]
#[repr(u8)]
enum IndiceInterrupcion {
    Timer = PIC_1_OFFSET,
    Teclado = PIC_1_OFFSET + 1,
    Raton = PIC_2_OFFSET + 4, // IRQ 12 (esclavo bit 4)
}

// PIT 8253/8254 reprogramado a 250 Hz (4 ms por tick) para respuesta instantanea de UI y red.
pub const PIT_HZ: u32 = 250;
pub const MS_POR_TICK: u64 = 4;

static TICKS: AtomicU64 = AtomicU64::new(0);

static TECLADO: Lazy<Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>>> = Lazy::new(|| {
    Mutex::new(Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::MapLettersToUnicode,
    ))
});

static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    idt.breakpoint.set_handler_fn(breakpoint);
    idt.page_fault.set_handler_fn(falla_de_pagina);
    idt.general_protection_fault.set_handler_fn(falla_proteccion);
    idt.invalid_opcode.set_handler_fn(instruccion_invalida);
    unsafe {
        idt.double_fault
            .set_handler_fn(doble_falla)
            .set_stack_index(gdt::PILA_DOBLE_FALLA);
    }
    // Registramos un manejador seguro para todas las IRQ del PIC (32..47) por si llega una IRQ espuria.
    for vec in PIC_1_OFFSET..PIC_1_OFFSET + 8 {
        idt[vec].set_handler_fn(manejar_irq_pic1_ignorado);
    }
    for vec in PIC_2_OFFSET..PIC_2_OFFSET + 8 {
        idt[vec].set_handler_fn(manejar_irq_pic2_ignorado);
    }
    idt[IndiceInterrupcion::Timer as u8].set_handler_fn(manejar_timer);
    idt[IndiceInterrupcion::Teclado as u8].set_handler_fn(manejar_teclado);
    idt[IndiceInterrupcion::Raton as u8].set_handler_fn(manejar_raton);
    idt
});

fn programar_pit_250hz() {
    let divisor: u16 = (1_193_182u32 / PIT_HZ) as u16;
    let mut cmd: Port<u8> = Port::new(0x43);
    let mut ch0: Port<u8> = Port::new(0x40);
    unsafe {
        cmd.write(0x36); // Canal 0, lo/hi byte, modo 3 (onda cuadrada)
        ch0.write((divisor & 0xFF) as u8);
        ch0.write((divisor >> 8) as u8);
    }
}

pub fn init() {
    IDT.load();
    programar_pit_250hz();
    unsafe {
        let mut pics = PICS.lock();
        pics.initialize();
        // Solo dejamos activas IRQ 0 (timer), IRQ 1 (teclado), IRQ 2 (cascada) e IRQ 12 (raton PS/2).
        pics.write_masks(0b1111_1000, 0b1110_1111);
    }
}

#[allow(dead_code)]
pub fn habilitar() {
    x86_64::instructions::interrupts::enable();
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

pub fn ms_transcurridos() -> u64 {
    ticks().saturating_mul(MS_POR_TICK)
}

extern "x86-interrupt" fn breakpoint(marco: InterruptStackFrame) {
    println!("EXCEPCION: BREAKPOINT en {:?}", marco.instruction_pointer);
}

extern "x86-interrupt" fn falla_de_pagina(marco: InterruptStackFrame, codigo: PageFaultErrorCode) {
    println!("EXCEPCION: FALLA DE PAGINA");
    println!("Direccion que se intento usar: {:?}", Cr2::read());
    println!("Tipo de error: {:?}", codigo);
    println!("Instruccion: {:?}", marco.instruction_pointer);
    loop {
        x86_64::instructions::hlt();
    }
}

extern "x86-interrupt" fn falla_proteccion(marco: InterruptStackFrame, codigo: u64) {
    panic!(
        "EXCEPCION: PROTECCION GENERAL (codigo={:#x})\nInstruccion: {:?}",
        codigo, marco.instruction_pointer
    );
}

extern "x86-interrupt" fn instruccion_invalida(marco: InterruptStackFrame) {
    panic!(
        "EXCEPCION: INSTRUCCION INVALIDA\nInstruccion: {:?}",
        marco.instruction_pointer
    );
}

extern "x86-interrupt" fn doble_falla(marco: InterruptStackFrame, _codigo: u64) -> ! {
    panic!(
        "EXCEPCION: DOBLE FALLA\nInstruccion: {:?}\nPila: {:?}",
        marco.instruction_pointer, marco.stack_pointer
    );
}

extern "x86-interrupt" fn manejar_irq_pic1_ignorado(_marco: InterruptStackFrame) {
    unsafe {
        PICS.lock().notify_end_of_interrupt(PIC_1_OFFSET + 7);
    }
}

extern "x86-interrupt" fn manejar_irq_pic2_ignorado(_marco: InterruptStackFrame) {
    unsafe {
        PICS.lock().notify_end_of_interrupt(PIC_2_OFFSET + 7);
    }
}

extern "x86-interrupt" fn manejar_timer(_marco: InterruptStackFrame) {
    TICKS.fetch_add(1, Ordering::Relaxed);
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(IndiceInterrupcion::Timer as u8);
    }
}

fn mapear_tecla_especial(code: KeyCode) -> Option<u16> {
    match code {
        KeyCode::ArrowUp => Some(entrada::TECLA_ARRIBA),
        KeyCode::ArrowDown => Some(entrada::TECLA_ABAJO),
        KeyCode::ArrowLeft => Some(entrada::TECLA_IZQ),
        KeyCode::ArrowRight => Some(entrada::TECLA_DER),
        KeyCode::Delete => Some(entrada::TECLA_SUPR),
        KeyCode::Home => Some(entrada::TECLA_INICIO),
        KeyCode::End => Some(entrada::TECLA_FIN),
        KeyCode::PageUp => Some(entrada::TECLA_PGUP),
        KeyCode::PageDown => Some(entrada::TECLA_PGDN),
        KeyCode::Escape => Some(27),
        KeyCode::Backspace => Some(8),
        KeyCode::Tab => Some(9),
        KeyCode::Return | KeyCode::NumpadEnter => Some(10),
        _ => None,
    }
}

extern "x86-interrupt" fn manejar_teclado(_marco: InterruptStackFrame) {
    let mut puerto = Port::new(0x60);
    let scancode: u8 = unsafe { puerto.read() };

    let mut kbd = TECLADO.lock();
    if let Ok(Some(evento)) = kbd.add_byte(scancode) {
        let es_arriba = evento.state == KeyState::Up;
        let code = evento.code;
        if let Some(tecla) = kbd.process_keyevent(evento) {
            match tecla {
                DecodedKey::Unicode(c) => {
                    if c.is_ascii() {
                        let b = c as u8;
                        teclado::empujar_byte(b);
                        entrada::empujar_tecla(true, b as u16);
                    }
                }
                DecodedKey::RawKey(k) => {
                    if let Some(esp) = mapear_tecla_especial(k) {
                        entrada::empujar_tecla(true, esp);
                    }
                }
            }
        } else if es_arriba {
            if let Some(esp) = mapear_tecla_especial(code) {
                entrada::empujar_tecla(false, esp);
            }
        }
    }

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(IndiceInterrupcion::Teclado as u8);
    }
}

extern "x86-interrupt" fn manejar_raton(_marco: InterruptStackFrame) {
    let mut p_estado: Port<u8> = Port::new(0x64);
    let mut p_datos: Port<u8> = Port::new(0x60);
    let estado = unsafe { p_estado.read() };
    if (estado & 0x20) != 0 {
        let byte = unsafe { p_datos.read() };
        raton::procesar_byte(byte);
    }
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(IndiceInterrupcion::Raton as u8);
    }
}
