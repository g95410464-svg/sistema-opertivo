fn main() {
    let args: Vec<String> = std::env::args().collect();
    println!("[Linux std/musl REAL] Hola desde fn main() con std::println! (args={:?})", args);
    match std::fs::read_to_string("/etc/os-release") {
        Ok(txt) => {
            let primera = txt.lines().next().unwrap_or("");
            println!("[Linux std/musl REAL] Leido /etc/os-release: {}", primera);
        }
        Err(e) => println!("[Linux std/musl REAL] Error leyendo: {}", e),
    }
}
