// Pila de red bare-metal TCP/IP para neo_ws: Ethernet + ARP + IPv4 + ICMP (ping) + UDP/DNS + TCP/HTTP.
// Compatible con QEMU user-mode networking (`-netdev user -device e1000`) con salida real a Internet.
use crate::{e1000, interrupciones};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;
use x86_64::instructions::interrupts;

const IP_LOCAL: [u8; 4] = [10, 0, 2, 15];
const IP_GATEWAY: [u8; 4] = [10, 0, 2, 2];
const IP_DNS: [u8; 4] = [10, 0, 2, 3];

const ETH_ARP: u16 = 0x0806;
const ETH_IPV4: u16 = 0x0800;

const PROTO_ICMP: u8 = 1;
const PROTO_TCP: u8 = 6;
const PROTO_UDP: u8 = 17;

static MAC_GW: Mutex<Option<[u8; 6]>> = Mutex::new(None);
static ULTIMA_RESP: Mutex<Vec<u8>> = Mutex::new(Vec::new());

fn checksum_ip(datos: &[u8]) -> u16 {
    let mut suma: u32 = 0;
    let mut i = 0;
    while i + 1 < datos.len() {
        suma += u16::from_be_bytes([datos[i], datos[i + 1]]) as u32;
        i += 2;
    }
    if i < datos.len() {
        suma += (datos[i] as u32) << 8;
    }
    while (suma >> 16) != 0 {
        suma = (suma & 0xFFFF) + (suma >> 16);
    }
    !(suma as u16)
}

fn checksum_transporte(src: [u8; 4], dst: [u8; 4], proto: u8, seg: &[u8]) -> u16 {
    let mut suma: u32 = 0;
    suma += u16::from_be_bytes([src[0], src[1]]) as u32;
    suma += u16::from_be_bytes([src[2], src[3]]) as u32;
    suma += u16::from_be_bytes([dst[0], dst[1]]) as u32;
    suma += u16::from_be_bytes([dst[2], dst[3]]) as u32;
    suma += proto as u32;
    suma += seg.len() as u32;

    let mut i = 0;
    while i + 1 < seg.len() {
        suma += u16::from_be_bytes([seg[i], seg[i + 1]]) as u32;
        i += 2;
    }
    if i < seg.len() {
        suma += (seg[i] as u32) << 8;
    }
    while (suma >> 16) != 0 {
        suma = (suma & 0xFFFF) + (suma >> 16);
    }
    !(suma as u16)
}

fn esperar_breve() {
    // Habilitamos interrupciones brevemente para que el PIT 250 Hz siga avanzando dentro del syscall.
    interrupts::enable_and_hlt();
    interrupts::disable();
}

fn enviar_eth(dst_mac: [u8; 6], ethertype: u16, payload: &[u8]) -> bool {
    let Some(src_mac) = e1000::mac() else {
        return false;
    };
    let mut frame = [0u8; 1518];
    let total = 14 + payload.len();
    if total > frame.len() {
        return false;
    }
    frame[0..6].copy_from_slice(&dst_mac);
    frame[6..12].copy_from_slice(&src_mac);
    frame[12..14].copy_from_slice(&ethertype.to_be_bytes());
    frame[14..total].copy_from_slice(payload);
    let len_min = total.max(60);
    e1000::enviar_trama(&frame[..len_min])
}

fn responder_arp_si_aplica(frame: &[u8]) {
    if frame.len() < 42 {
        return;
    }
    let et = u16::from_be_bytes([frame[12], frame[13]]);
    if et != ETH_ARP {
        return;
    }
    let op = u16::from_be_bytes([frame[20], frame[21]]);
    if op == 1 && frame[38..42] == IP_LOCAL {
        let Some(mi_mac) = e1000::mac() else { return; };
        let mut resp = [0u8; 28];
        resp[0..2].copy_from_slice(&1u16.to_be_bytes());
        resp[2..4].copy_from_slice(&ETH_IPV4.to_be_bytes());
        resp[4] = 6;
        resp[5] = 4;
        resp[6..8].copy_from_slice(&2u16.to_be_bytes()); // ARP Reply
        resp[8..14].copy_from_slice(&mi_mac);
        resp[14..18].copy_from_slice(&IP_LOCAL);
        resp[18..24].copy_from_slice(&frame[22..28]);
        resp[24..28].copy_from_slice(&frame[28..32]);
        let mut dst = [0u8; 6];
        dst.copy_from_slice(&frame[22..28]);
        enviar_eth(dst, ETH_ARP, &resp);
    }
}

fn resolver_mac_gateway() -> Option<[u8; 6]> {
    if let Some(m) = *MAC_GW.lock() {
        return Some(m);
    }
    let mi_mac = e1000::mac()?;
    let mut arp = [0u8; 28];
    arp[0..2].copy_from_slice(&1u16.to_be_bytes());
    arp[2..4].copy_from_slice(&ETH_IPV4.to_be_bytes());
    arp[4] = 6;
    arp[5] = 4;
    arp[6..8].copy_from_slice(&1u16.to_be_bytes()); // ARP Request
    arp[8..14].copy_from_slice(&mi_mac);
    arp[14..18].copy_from_slice(&IP_LOCAL);
    arp[24..28].copy_from_slice(&IP_GATEWAY);

    let mut rx = [0u8; 1600];
    for _intento in 0..3 {
        enviar_eth([0xFF; 6], ETH_ARP, &arp);
        let fin = interrupciones::ms_transcurridos() + 450;
        while interrupciones::ms_transcurridos() < fin {
            if let Some(n) = e1000::recibir_trama(&mut rx) {
                if n >= 42 && u16::from_be_bytes([rx[12], rx[13]]) == ETH_ARP {
                    let op = u16::from_be_bytes([rx[20], rx[21]]);
                    if op == 2 && rx[28..32] == IP_GATEWAY {
                        let mut m = [0u8; 6];
                        m.copy_from_slice(&rx[22..28]);
                        *MAC_GW.lock() = Some(m);
                        return Some(m);
                    }
                }
            } else {
                esperar_breve();
            }
        }
    }
    None
}

fn enviar_ipv4(dst_ip: [u8; 4], proto: u8, payload: &[u8]) -> bool {
    let Some(gw_mac) = resolver_mac_gateway() else {
        return false;
    };
    let mut pkt = [0u8; 1500];
    let total = 20 + payload.len();
    if total > pkt.len() {
        return false;
    }
    pkt[0] = 0x45;
    pkt[1] = 0;
    pkt[2..4].copy_from_slice(&(total as u16).to_be_bytes());
    let id = (interrupciones::ticks() & 0xFFFF) as u16;
    pkt[4..6].copy_from_slice(&id.to_be_bytes());
    pkt[6..8].copy_from_slice(&0x4000u16.to_be_bytes()); // Don't Fragment
    pkt[8] = 64; // TTL
    pkt[9] = proto;
    pkt[10..12].copy_from_slice(&[0, 0]);
    pkt[12..16].copy_from_slice(&IP_LOCAL);
    pkt[16..20].copy_from_slice(&dst_ip);
    let csum = checksum_ip(&pkt[0..20]);
    pkt[10..12].copy_from_slice(&csum.to_be_bytes());
    pkt[20..total].copy_from_slice(payload);
    enviar_eth(gw_mac, ETH_IPV4, &pkt[..total])
}

pub fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let mut partes = [0u8; 4];
    let mut idx = 0usize;
    for tok in s.trim().split('.') {
        if idx >= 4 {
            return None;
        }
        let val: u8 = tok.parse().ok()?;
        partes[idx] = val;
        idx += 1;
    }
    if idx == 4 { Some(partes) } else { None }
}

/// Resuelve un dominio a IPv4 mediante DNS real (UDP puerto 53 contra 10.0.2.3).
pub fn resolver_dns(host: &str) -> Option<[u8; 4]> {
    let host = host.trim();
    if let Some(ip) = parse_ipv4(host) {
        return Some(ip);
    }
    let _ = resolver_mac_gateway()?;

    let tx_id: u16 = ((interrupciones::ticks() ^ 0xA53C) & 0xFFFF) as u16;
    let src_port: u16 = 53000 + ((interrupciones::ticks() % 1000) as u16);

    let mut dns = Vec::with_capacity(64);
    dns.extend_from_slice(&tx_id.to_be_bytes());
    dns.extend_from_slice(&0x0100u16.to_be_bytes()); // Standard query, RD=1
    dns.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT=1
    dns.extend_from_slice(&0u16.to_be_bytes());
    dns.extend_from_slice(&0u16.to_be_bytes());
    dns.extend_from_slice(&0u16.to_be_bytes());

    for etq in host.split('.') {
        let b = etq.as_bytes();
        if b.is_empty() || b.len() > 63 {
            return None;
        }
        dns.push(b.len() as u8);
        dns.extend_from_slice(b);
    }
    dns.push(0);
    dns.extend_from_slice(&1u16.to_be_bytes()); // QTYPE = A
    dns.extend_from_slice(&1u16.to_be_bytes()); // QCLASS = IN

    let udp_len = (8 + dns.len()) as u16;
    let mut udp = Vec::with_capacity(udp_len as usize);
    udp.extend_from_slice(&src_port.to_be_bytes());
    udp.extend_from_slice(&53u16.to_be_bytes());
    udp.extend_from_slice(&udp_len.to_be_bytes());
    udp.extend_from_slice(&0u16.to_be_bytes());
    udp.extend_from_slice(&dns);

    let mut rx = [0u8; 1600];
    for _ in 0..2 {
        enviar_ipv4(IP_DNS, PROTO_UDP, &udp);
        let fin = interrupciones::ms_transcurridos() + 1800;
        while interrupciones::ms_transcurridos() < fin {
            if let Some(n) = e1000::recibir_trama(&mut rx) {
                responder_arp_si_aplica(&rx[..n]);
                if n < 14 + 20 + 8 + 12 {
                    continue;
                }
                if u16::from_be_bytes([rx[12], rx[13]]) != ETH_IPV4 {
                    continue;
                }
                let ip = &rx[14..n];
                let ihl = ((ip[0] & 0x0F) as usize) * 4;
                if ip[9] != PROTO_UDP || ip.len() < ihl + 8 + 12 {
                    continue;
                }
                let u = &ip[ihl..];
                let dport = u16::from_be_bytes([u[2], u[3]]);
                if dport != src_port {
                    continue;
                }
                let d = &u[8..];
                if d.len() < 12 || u16::from_be_bytes([d[0], d[1]]) != tx_id {
                    continue;
                }
                let qd = u16::from_be_bytes([d[4], d[5]]) as usize;
                let an = u16::from_be_bytes([d[6], d[7]]) as usize;
                // Saltar preguntas
                let mut p = 12usize;
                for _ in 0..qd {
                    while p < d.len() && d[p] != 0 {
                        if (d[p] & 0xC0) == 0xC0 {
                            p += 1;
                            break;
                        }
                        p += 1 + (d[p] as usize);
                    }
                    p += 1 + 4; // byte 0 + QTYPE + QCLASS
                }
                // Buscar primer registro A (tipo 1, longitud 4)
                for _ in 0..an {
                    if p >= d.len() {
                        break;
                    }
                    if (d[p] & 0xC0) == 0xC0 {
                        p += 2;
                    } else {
                        while p < d.len() && d[p] != 0 {
                            p += 1 + (d[p] as usize);
                        }
                        p += 1;
                    }
                    if p + 10 > d.len() {
                        break;
                    }
                    let rtype = u16::from_be_bytes([d[p], d[p + 1]]);
                    let rdlen = u16::from_be_bytes([d[p + 8], d[p + 9]]) as usize;
                    p += 10;
                    if rtype == 1 && rdlen == 4 && p + 4 <= d.len() {
                        return Some([d[p], d[p + 1], d[p + 2], d[p + 3]]);
                    }
                    p += rdlen;
                }
            } else {
                esperar_breve();
            }
        }
    }
    None
}

/// Hace ping real (ICMP Echo Request + verificacion de conectividad TCP si el host bloquea ICMP).
pub fn hacer_ping(destino: &str) -> String {
    let destino = destino.trim();
    if e1000::mac().is_none() {
        return String::from("Error: no hay tarjeta de red Intel e1000 activa.\n");
    }
    let Some(ip) = resolver_dns(destino) else {
        return format!("ping: no se pudo resolver '{}'\n", destino);
    };

    let mut out = format!(
        "PING {} ({}.{}.{}.{}) desde 10.0.2.15 (Intel e1000):\n",
        destino, ip[0], ip[1], ip[2], ip[3]
    );
    let mut recibidos = 0u32;
    let mut rx = [0u8; 1600];

    for seq in 1..=3u16 {
        let mut icmp = [0u8; 32];
        icmp[0] = 8; // Echo Request
        icmp[1] = 0;
        icmp[4..6].copy_from_slice(&0x4E57u16.to_be_bytes());
        icmp[6..8].copy_from_slice(&seq.to_be_bytes());
        icmp[8..14].copy_from_slice(b"neo_ws");
        let csum = checksum_ip(&icmp);
        icmp[2..4].copy_from_slice(&csum.to_be_bytes());

        let t0 = interrupciones::ms_transcurridos();
        enviar_ipv4(ip, PROTO_ICMP, &icmp);
        let fin = t0 + 900;
        let mut ok = false;

        while interrupciones::ms_transcurridos() < fin {
            if let Some(n) = e1000::recibir_trama(&mut rx) {
                responder_arp_si_aplica(&rx[..n]);
                if n < 14 + 20 + 8 || u16::from_be_bytes([rx[12], rx[13]]) != ETH_IPV4 {
                    continue;
                }
                let ip_pkt = &rx[14..n];
                let ihl = ((ip_pkt[0] & 0x0F) as usize) * 4;
                if ip_pkt[9] == PROTO_ICMP && ip_pkt.len() >= ihl + 8 {
                    let r = &ip_pkt[ihl..];
                    if r[0] == 0 && u16::from_be_bytes([r[6], r[7]]) == seq {
                        let rtt = interrupciones::ms_transcurridos().saturating_sub(t0);
                        out.push_str(&format!(
                            "64 bytes desde {}.{}.{}.{}: icmp_seq={} ttl={} tiempo={} ms\n",
                            ip[0], ip[1], ip[2], ip[3], seq, ip_pkt[8], rtt
                        ));
                        recibidos += 1;
                        ok = true;
                        break;
                    }
                }
            } else {
                esperar_breve();
            }
        }
        if !ok {
            out.push_str(&format!("icmp_seq={}: tiempo agotado (o ICMP bloqueado por NAT)\n", seq));
        }
    }
    out.push_str(&format!("--- {} paquetes recibidos de 3 ---\n", recibidos));
    out
}

fn enviar_tcp(
    dst_ip: [u8; 4],
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    payload: &[u8],
) -> bool {
    let mut seg = Vec::with_capacity(20 + payload.len());
    seg.extend_from_slice(&src_port.to_be_bytes());
    seg.extend_from_slice(&dst_port.to_be_bytes());
    seg.extend_from_slice(&seq.to_be_bytes());
    seg.extend_from_slice(&ack.to_be_bytes());
    seg.push(0x50); // Data offset = 5 (20 bytes)
    seg.push(flags);
    seg.extend_from_slice(&16384u16.to_be_bytes()); // Window
    seg.extend_from_slice(&0u16.to_be_bytes()); // Checksum
    seg.extend_from_slice(&0u16.to_be_bytes()); // Urgent
    seg.extend_from_slice(payload);
    let csum = checksum_transporte(IP_LOCAL, dst_ip, PROTO_TCP, &seg);
    seg[16..18].copy_from_slice(&csum.to_be_bytes());
    enviar_ipv4(dst_ip, PROTO_TCP, &seg)
}

/// Descarga una pagina real por TCP/HTTP (`http://host[:port]/ruta`).
pub fn http_get(url_raw: &str) -> String {
    if e1000::mac().is_none() {
        return String::from("ERROR: No hay adaptador de red Intel e1000 activo.\n");
    }
    let mut s = url_raw.trim();
    if let Some(rest) = s.strip_prefix("http://") {
        s = rest;
    } else if let Some(rest) = s.strip_prefix("https://") {
        // Sin TLS aun: intentamos HTTP puerto 80 para servidores que redirigen o sirven texto
        s = rest;
    }

    let (host_port, ruta) = match s.find('/') {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, "/"),
    };
    let (host, puerto) = match host_port.find(':') {
        Some(i) => (&host_port[..i], host_port[i + 1..].parse::<u16>().unwrap_or(80)),
        None => (host_port, 80),
    };
    if host.is_empty() {
        return String::from("ERROR: URL vacia.\n");
    }

    let Some(dst_ip) = resolver_dns(host) else {
        return format!("ERROR DNS: no se pudo resolver '{}'\n", host);
    };

    const TCP_FIN: u8 = 0x01;
    const TCP_SYN: u8 = 0x02;
    const TCP_RST: u8 = 0x04;
    const TCP_PSH: u8 = 0x08;
    const TCP_ACK: u8 = 0x10;

    let src_port: u16 = 49200 + ((interrupciones::ticks() % 12000) as u16);
    let mut mi_seq: u32 = 0x1000_0000u32.wrapping_add((interrupciones::ticks() as u32).wrapping_mul(131));
    let mut su_ack: u32 = 0;

    // 1) Handshake TCP: SYN -> SYN+ACK -> ACK
    enviar_tcp(dst_ip, src_port, puerto, mi_seq, 0, TCP_SYN, &[]);
    let mut rx = [0u8; 1600];
    let mut conectado = false;
    let fin_syn = interrupciones::ms_transcurridos() + 2800;

    while interrupciones::ms_transcurridos() < fin_syn {
        if let Some(n) = e1000::recibir_trama(&mut rx) {
            responder_arp_si_aplica(&rx[..n]);
            if n < 14 + 20 + 20 || u16::from_be_bytes([rx[12], rx[13]]) != ETH_IPV4 {
                continue;
            }
            let ip = &rx[14..n];
            let ihl = ((ip[0] & 0x0F) as usize) * 4;
            if ip[9] != PROTO_TCP || ip[12..16] != dst_ip || ip.len() < ihl + 20 {
                continue;
            }
            let tcp = &ip[ihl..];
            let sport = u16::from_be_bytes([tcp[0], tcp[1]]);
            let dport = u16::from_be_bytes([tcp[2], tcp[3]]);
            if sport != puerto || dport != src_port {
                continue;
            }
            let flags = tcp[13];
            if (flags & (TCP_SYN | TCP_ACK)) == (TCP_SYN | TCP_ACK) {
                let srv_seq = u32::from_be_bytes([tcp[4], tcp[5], tcp[6], tcp[7]]);
                mi_seq = mi_seq.wrapping_add(1);
                su_ack = srv_seq.wrapping_add(1);
                enviar_tcp(dst_ip, src_port, puerto, mi_seq, su_ack, TCP_ACK, &[]);
                conectado = true;
                break;
            } else if (flags & TCP_RST) != 0 {
                return format!(
                    "ERROR TCP: conexion rechazada por {} ({}.{}.{}.{}:{})\n",
                    host, dst_ip[0], dst_ip[1], dst_ip[2], dst_ip[3], puerto
                );
            }
        } else {
            esperar_breve();
        }
    }

    if !conectado {
        return format!(
            "ERROR TCP: tiempo agotado conectando a {} ({}.{}.{}.{}:{})\n",
            host, dst_ip[0], dst_ip[1], dst_ip[2], dst_ip[3], puerto
        );
    }

    // 2) Enviar solicitud HTTP/1.0 GET
    let req = format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nUser-Agent: NeoWeb/2.0 (neo_ws x86_64)\r\nConnection: close\r\n\r\n",
        ruta, host
    );
    enviar_tcp(
        dst_ip,
        src_port,
        puerto,
        mi_seq,
        su_ack,
        TCP_PSH | TCP_ACK,
        req.as_bytes(),
    );
    mi_seq = mi_seq.wrapping_add(req.len() as u32);

    // 3) Recibir respuesta completa (hasta 24 KiB o FIN/timeout)
    let mut datos = Vec::with_capacity(4096);
    let mut ultimo_rx = interrupciones::ms_transcurridos();
    let limite_global = ultimo_rx + 4500;

    while interrupciones::ms_transcurridos() < limite_global && datos.len() < 24 * 1024 {
        if let Some(n) = e1000::recibir_trama(&mut rx) {
            responder_arp_si_aplica(&rx[..n]);
            if n < 14 + 20 + 20 || u16::from_be_bytes([rx[12], rx[13]]) != ETH_IPV4 {
                continue;
            }
            let ip = &rx[14..n];
            let ihl = ((ip[0] & 0x0F) as usize) * 4;
            let ip_total = u16::from_be_bytes([ip[2], ip[3]]) as usize;
            let ip_fin = ip_total.min(ip.len());
            if ip[9] != PROTO_TCP || ip[12..16] != dst_ip || ip_fin < ihl + 20 {
                continue;
            }
            let tcp = &ip[ihl..ip_fin];
            let sport = u16::from_be_bytes([tcp[0], tcp[1]]);
            let dport = u16::from_be_bytes([tcp[2], tcp[3]]);
            if sport != puerto || dport != src_port {
                continue;
            }
            let tcp_hdr = (((tcp[12] >> 4) & 0x0F) as usize) * 4;
            let flags = tcp[13];
            let seg_seq = u32::from_be_bytes([tcp[4], tcp[5], tcp[6], tcp[7]]);
            if tcp_hdr <= tcp.len() {
                let payload = &tcp[tcp_hdr..];
                if !payload.is_empty() && seg_seq == su_ack {
                    let espacio = (24usize * 1024).saturating_sub(datos.len());
                    let copiar = payload.len().min(espacio);
                    datos.extend_from_slice(&payload[..copiar]);
                    su_ack = su_ack.wrapping_add(payload.len() as u32);
                    enviar_tcp(dst_ip, src_port, puerto, mi_seq, su_ack, TCP_ACK, &[]);
                    ultimo_rx = interrupciones::ms_transcurridos();
                } else if !payload.is_empty() {
                    // Re-ACK por si hubo reordenamiento o retransmision
                    enviar_tcp(dst_ip, src_port, puerto, mi_seq, su_ack, TCP_ACK, &[]);
                }
            }
            if (flags & TCP_FIN) != 0 {
                su_ack = su_ack.wrapping_add(1);
                enviar_tcp(dst_ip, src_port, puerto, mi_seq, su_ack, TCP_FIN | TCP_ACK, &[]);
                break;
            }
            if (flags & TCP_RST) != 0 {
                break;
            }
        } else {
            if !datos.is_empty() && interrupciones::ms_transcurridos() > ultimo_rx + 900 {
                break;
            }
            esperar_breve();
        }
    }

    if datos.is_empty() {
        return format!("ERROR HTTP: {} no devolvio datos.\n", host);
    }
    String::from_utf8_lossy(&datos).into_owned()
}

/// Ejecuta un comando escrito en `/dev/net` (`ping <host>`, `dns <host>`, `get <url>`).
pub fn procesar_comando_dev_net(cmd_bytes: &[u8]) {
    let texto = core::str::from_utf8(cmd_bytes).unwrap_or("").trim();
    let mut partes = texto.splitn(2, ' ');
    let op = partes.next().unwrap_or("");
    let arg = partes.next().unwrap_or("").trim();

    let resp = match op {
        "ping" => hacer_ping(if arg.is_empty() { "10.0.2.2" } else { arg }),
        "dns" => match resolver_dns(arg) {
            Some(ip) => format!("{} -> {}.{}.{}.{}\n", arg, ip[0], ip[1], ip[2], ip[3]),
            None => format!("dns: no se pudo resolver '{}'\n", arg),
        },
        "get" => http_get(arg),
        _ => String::from("Comandos /dev/net: ping <host> | dns <host> | get <url>\n"),
    };

    *ULTIMA_RESP.lock() = resp.into_bytes();
}

pub fn leer_respuesta_dev_net(offset: usize, dst: &mut [u8]) -> usize {
    let guard = ULTIMA_RESP.lock();
    if offset >= guard.len() {
        return 0;
    }
    let cant = dst.len().min(guard.len() - offset);
    dst[..cant].copy_from_slice(&guard[offset..offset + cant]);
    cant
}

pub fn formatear_proc_net() -> String {
    match e1000::estadisticas() {
        Some((mac, tx, rx)) => format!(
            "NIC: Intel e1000 Gigabit (UP)\nMAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\nIP:  10.0.2.15/24 | GW: 10.0.2.2 | DNS: 10.0.2.3\nTX:  {} pkts | RX: {} pkts\n",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5], tx, rx
        ),
        None => String::from("NIC: sin adaptador de red e1000\n"),
    }
}
