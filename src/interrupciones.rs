// Manejo de excepciones del CPU e interrupciones de hardware (PIC, timer, teclado).
use crate::{gdt, print, println};
use core::sync::atomic::{AtomicU64, Ordering};
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
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
    Teclado,
}

static TICKS: AtomicU64 = AtomicU64::new(0);

static TECLADO: Lazy<Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>>> = Lazy::new(|| {
    Mutex::new(Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    ))
});

static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    idt.breakpoint.set_handler_fn(breakpoint);
    idt.page_fault.set_handler_fn(falla_de_pagina);
    unsafe {
        idt.double_fault
            .set_handler_fn(doble_falla)
            .set_stack_index(gdt::PILA_DOBLE_FALLA);
    }
    idt[IndiceInterrupcion::Timer as u8].set_handler_fn(manejar_timer);
    idt[IndiceInterrupcion::Teclado as u8].set_handler_fn(manejar_teclado);
    idt
});

pub fn init() {
    IDT.load();
    unsafe {
        PICS.lock().initialize();
    }
}

pub fn habilitar() {
    x86_64::instructions::interrupts::enable();
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
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

extern "x86-interrupt" fn doble_falla(marco: InterruptStackFrame, _codigo: u64) -> ! {
    panic!(
        "EXCEPCION: DOBLE FALLA\nInstruccion: {:?}\nPila: {:?}",
        marco.instruction_pointer, marco.stack_pointer
    );
}

extern "x86-interrupt" fn manejar_timer(_marco: InterruptStackFrame) {
    TICKS.fetch_add(1, Ordering::Relaxed);
    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(IndiceInterrupcion::Timer as u8);
    }
}

extern "x86-interrupt" fn manejar_teclado(_marco: InterruptStackFrame) {
    let mut puerto = Port::new(0x60);
    let scancode: u8 = unsafe { puerto.read() };

    let mut teclado = TECLADO.lock();
    if let Ok(Some(evento)) = teclado.add_byte(scancode) {
        if let Some(tecla) = teclado.process_keyevent(evento) {
            if let DecodedKey::Unicode(caracter) = tecla {
                print!("{}", caracter);
            }
        }
    }

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(IndiceInterrupcion::Teclado as u8);
    }
}
