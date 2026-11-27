#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

mod elf;
mod fs;
mod gdt;
mod heap;
mod interrupciones;
mod memoria;
mod syscall;
mod teclado;
mod vga;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use vga::Color;
use x86_64::VirtAddr;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    vga::limpiar();
    println!("Iniciando neo_ws...");

    gdt::init();
    interrupciones::init();
    syscall::init();

    // Inicializamos paginacion y heap del kernel (1 MiB).
    let ram_mb = memoria::ram_usable_kb(&boot_info.memory_map) / 1024;
    let offset_fisico = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memoria::init(offset_fisico) };
    let mut frames = unsafe { memoria::AsignadorFrames::init(&boot_info.memory_map) };

    heap::init(&mut mapper, &mut frames).expect("Fallo al inicializar el heap");
    memoria::registrar_global(mapper, frames, offset_fisico);

    // Montamos el sistema de archivos en memoria (ramfs).
    fs::init();
    println!(
        "Memoria: {} MB RAM | Heap: {} KiB | ramfs montado",
        ram_mb,
        heap::TAMANO_HEAP / 1024
    );

    // Cargamos /bin/sh directamente desde nuestro sistema de archivos y lo ejecutamos en Ring 3.
    let binario_sh = fs::leer_archivo_kernel("/bin/sh").expect("Falta /bin/sh en ramfs");
    let programa = elf::cargar(&binario_sh).expect("Error al cargar /bin/sh");
    println!(
        "Lanzando /bin/sh en Ring 3 ({} segs, entry={:#x}, brk={:#x})...\n",
        programa.segmentos_cargados, programa.punto_entrada, programa.fin_datos
    );

    let codigo = syscall::ejecutar_en_usuario(
        programa.punto_entrada,
        programa.pila_usuario,
        programa.fin_datos,
    );

    vga::color(Color::VerdeClaro, Color::Negro);
    println!("\n[neo_ws] /bin/sh termino con codigo: {}", codigo);
    vga::color(Color::Blanco, Color::Negro);
    println!("Sistema detenido de forma segura. Puedes cerrar QEMU.");

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
