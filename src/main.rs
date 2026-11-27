#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

mod gdt;
mod interrupciones;
mod vga;

use core::panic::PanicInfo;
use vga::Color;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    vga::limpiar();
    println!("Bienvenido a neo_ws!");

    gdt::init();
    interrupciones::init();
    interrupciones::habilitar();

    // Esperamos el primer pulso del reloj (timer) para confirmar que funciona.
    while interrupciones::ticks() == 0 {
        x86_64::instructions::hlt();
    }

    vga::color(Color::VerdeClaro, Color::Negro);
    println!("Timer y teclado activos! Haz clic en la ventana y escribe:");
    vga::color(Color::Blanco, Color::Negro);

    hlt_loop();
}

pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    x86_64::instructions::interrupts::disable();
    vga::color(Color::Blanco, Color::Rojo);
    println!("{}", info);
    hlt_loop();
}
