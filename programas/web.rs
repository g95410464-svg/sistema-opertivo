// NeoWeb 2.0 — Navegador Web minimalista nativo en Ring 3 para neo_ws.
// Usa `/dev/net` (Intel e1000 + DNS + TCP + HTTP) para abrir paginas reales de Internet
// y renderiza HTML (titulos, parrafos, listas, enlaces) con fuentes anti-aliased suaves.
use crate::gfx::Lienzo;

const MAX_LINEAS: usize = 120;
const MAX_COLS: usize = 86;

pub const LINEA_TEXTO: u8 = 0;
pub const LINEA_TITULO: u8 = 1;
pub const LINEA_ENLACE: u8 = 2;
pub const LINEA_META: u8 = 3;

pub struct Navegador {
    pub url: [u8; 128],
    pub url_len: usize,
    pub editando_url: bool,
    pub titulo: [u8; 96],
    pub titulo_len: usize,
    pub estado: [u8; 80],
    pub estado_len: usize,
    pub lineas: [[u8; MAX_COLS]; MAX_LINEAS],
    pub lineas_len: [usize; MAX_LINEAS],
    pub lineas_tipo: [u8; MAX_LINEAS],
    pub num_lineas: usize,
    pub scroll: usize,
    pub raw_buf: [u8; 20 * 1024],
}

impl Navegador {
    pub fn inicializar(&mut self) {
        self.url_len = 0;
        self.editando_url = false;
        self.titulo_len = 0;
        self.estado_len = 0;
        self.num_lineas = 0;
        self.scroll = 0;
        self.cargar_home(b"");
    }

    pub fn establecer_url(&mut self, u: &[u8]) {
        let n = u.len().min(self.url.len() - 1);
        self.url[..n].copy_from_slice(&u[..n]);
        self.url_len = n;
    }

    fn establecer_estado(&mut self, s: &[u8]) {
        let n = s.len().min(self.estado.len());
        self.estado[..n].copy_from_slice(&s[..n]);
        self.estado_len = n;
    }

    fn establecer_titulo(&mut self, s: &[u8]) {
        let n = s.len().min(self.titulo.len());
        self.titulo[..n].copy_from_slice(&s[..n]);
        self.titulo_len = n;
    }

    fn limpiar_lineas(&mut self) {
        self.num_lineas = 0;
        self.scroll = 0;
    }

    fn empujar_linea(&mut self, s: &[u8], tipo: u8) {
        if self.num_lineas >= MAX_LINEAS {
            return;
        }
        let n = s.len().min(MAX_COLS);
        let idx = self.num_lineas;
        self.lineas[idx][..n].copy_from_slice(&s[..n]);
        self.lineas_len[idx] = n;
        self.lineas_tipo[idx] = tipo;
        self.num_lineas += 1;
    }

    fn empujar_envuelto(&mut self, texto: &[u8], tipo: u8) {
        let mut s = texto;
        while let Some(&b) = s.first() {
            if b <= 32 { s = &s[1..]; } else { break; }
        }
        while let Some(&b) = s.last() {
            if b <= 32 { s = &s[..s.len() - 1]; } else { break; }
        }
        if s.is_empty() {
            return;
        }
        let max_c = if tipo == LINEA_TITULO { 68 } else { 82 };
        let mut off = 0usize;
        while off < s.len() && self.num_lineas < MAX_LINEAS {
            let mut fin = (off + max_c).min(s.len());
            if fin < s.len() {
                let mut corte = fin;
                while corte > off + 20 && s[corte] != b' ' {
                    corte -= 1;
                }
                if s[corte] == b' ' {
                    fin = corte;
                }
            }
            self.empujar_linea(&s[off..fin], tipo);
            off = fin;
            while off < s.len() && s[off] == b' ' {
                off += 1;
            }
        }
    }

    pub fn cargar_home(&mut self, info_net: &[u8]) {
        self.establecer_url(b"neo://home");
        self.establecer_titulo(b"NeoWeb 2.0  -  Inicio");
        self.establecer_estado(b"Listo  |  Intel e1000 Gigabit + DNS + TCP/HTTP");
        self.limpiar_lineas();

        self.empujar_linea(b"Bienvenido a NeoWeb 2.0", LINEA_TITULO);
        self.empujar_linea(b"", LINEA_TEXTO);
        self.empujar_linea(
            b"Navegador nativo en Ring 3 conectado a Internet real mediante el driver PCI",
            LINEA_TEXTO,
        );
        self.empujar_linea(
            b"Intel e1000, ARP, IPv4, UDP/DNS y TCP/HTTP escritos desde cero en Rust.",
            LINEA_TEXTO,
        );
        self.empujar_linea(b"", LINEA_TEXTO);
        self.empujar_linea(b"Marcadores rapidos (haz clic arriba o escribe una URL):", LINEA_TITULO);
        self.empujar_linea(b"  * http://example.com/       - Pagina de prueba oficial IANA", LINEA_ENLACE);
        self.empujar_linea(b"  * http://info.cern.ch/      - El primer sitio web de la historia (CERN)", LINEA_ENLACE);
        self.empujar_linea(b"  * http://neverssl.com/      - Sitio informativo HTTP puro", LINEA_ENLACE);
        self.empujar_linea(b"", LINEA_TEXTO);
        self.empujar_linea(b"Estado de la red (/proc/net):", LINEA_META);

        let mut ini = 0usize;
        for i in 0..=info_net.len() {
            if i == info_net.len() || info_net[i] == b'\n' {
                if i > ini {
                    self.empujar_linea(&info_net[ini..i], LINEA_META);
                }
                ini = i + 1;
            }
        }
    }

    /// Navega a la URL actual llamando al callback `ejecutar_get(url, dst)` que usa `/dev/net`.
    pub fn navegar<F>(&mut self, info_net: &[u8], ejecutar_get: F)
    where
        F: FnOnce(&[u8], &mut [u8]) -> usize,
    {
        if self.url_len >= 6 && &self.url[..6] == b"neo://" {
            self.cargar_home(info_net);
            return;
        }
        let mut url_copia = [0u8; 128];
        let ul = self.url_len;
        url_copia[..ul].copy_from_slice(&self.url[..ul]);

        let n = ejecutar_get(&url_copia[..ul], &mut self.raw_buf);
        self.limpiar_lineas();
        if n == 0 {
            self.establecer_titulo(b"Sin respuesta");
            self.establecer_estado(b"Error: sin respuesta de /dev/net");
            self.empujar_linea(b"No se recibieron datos del servidor.", LINEA_TITULO);
            return;
        }

        // Si empieza con ERROR, mostrar el mensaje directamente
        if n >= 5 && &self.raw_buf[..5] == b"ERROR" {
            self.establecer_titulo(b"Error de conexion");
            self.establecer_estado(b"Fallo al conectar");
            self.empujar_linea(b"No se pudo cargar la pagina", LINEA_TITULO);
            self.empujar_linea(b"", LINEA_TEXTO);
            let mut copy_err = [0u8; 256];
            let el = n.min(256);
            copy_err[..el].copy_from_slice(&self.raw_buf[..el]);
            self.empujar_envuelto(&copy_err[..el], LINEA_META);
            return;
        }

        // Separar cabeceras HTTP del cuerpo HTML
        let mut cuerpo_ini = 0usize;
        let mut primera_linea_fin = n.min(70);
        for i in 0..n {
            if self.raw_buf[i] == b'\r' || self.raw_buf[i] == b'\n' {
                primera_linea_fin = i.min(70);
                break;
            }
        }
        for i in 0..n.saturating_sub(3) {
            if &self.raw_buf[i..i + 4] == b"\r\n\r\n" {
                cuerpo_ini = i + 4;
                break;
            }
        }
        if cuerpo_ini == 0 {
            for i in 0..n.saturating_sub(1) {
                if &self.raw_buf[i..i + 2] == b"\n\n" {
                    cuerpo_ini = i + 2;
                    break;
                }
            }
        }

        let mut est = [0u8; 80];
        let el = if cuerpo_ini > 0 {
            let l = primera_linea_fin.min(54);
            est[..l].copy_from_slice(&self.raw_buf[..l]);
            let suf = b"  |  OK (e1000)";
            let sl = suf.len().min(est.len() - l);
            est[l..l + sl].copy_from_slice(&suf[..sl]);
            l + sl
        } else {
            let s = b"Contenido cargado";
            est[..s.len()].copy_from_slice(s);
            s.len()
        };
        self.establecer_estado(&est[..el]);
        self.establecer_titulo(&url_copia[..ul]);

        self.parsear_html(cuerpo_ini, n);
        if self.num_lineas == 0 {
            self.empujar_linea(b"(Pagina sin texto visible o redireccion HTTPS)", LINEA_META);
            if cuerpo_ini > 0 {
                let mut hdr = [0u8; 256];
                let hl = cuerpo_ini.min(256);
                hdr[..hl].copy_from_slice(&self.raw_buf[..hl]);
                self.empujar_envuelto(&hdr[..hl], LINEA_META);
            }
        }
    }

    fn parsear_html(&mut self, inicio: usize, fin: usize) {
        let mut i = inicio;
        let mut en_script = false;
        let mut en_style = false;
        let mut en_title = false;
        let mut tipo_actual = LINEA_TEXTO;

        let mut parrafo = [0u8; 512];
        let mut p_len = 0usize;
        let mut tit_buf = [0u8; 96];
        let mut tit_len = 0usize;

        while i < fin && self.num_lineas < MAX_LINEAS {
            let b = self.raw_buf[i];
            if b == b'<' {
                let tag_ini = i + 1;
                let mut tag_fin = tag_ini;
                while tag_fin < fin && self.raw_buf[tag_fin] != b'>' {
                    tag_fin += 1;
                }
                let mut tag = [0u8; 20];
                let mut tl = 0usize;
                for k in tag_ini..tag_fin {
                    let c = self.raw_buf[k];
                    if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
                        break;
                    }
                    if tl < tag.len() {
                        tag[tl] = if c >= b'A' && c <= b'Z' { c + 32 } else { c };
                        tl += 1;
                    }
                }
                let t = &tag[..tl];

                if t == b"script" { en_script = true; }
                else if t == b"/script" { en_script = false; }
                else if t == b"style" { en_style = true; }
                else if t == b"/style" { en_style = false; }
                else if t == b"title" { en_title = true; tit_len = 0; }
                else if t == b"/title" {
                    en_title = false;
                    if tit_len > 0 {
                        self.establecer_titulo(&tit_buf[..tit_len]);
                    }
                } else if !en_script && !en_style {
                    let es_bloque = matches!(
                        t,
                        b"p" | b"/p" | b"div" | b"/div" | b"br" | b"br/" | b"li" | b"/li"
                            | b"h1" | b"/h1" | b"h2" | b"/h2" | b"h3" | b"/h3" | b"tr" | b"hr"
                    );
                    if es_bloque && p_len > 0 {
                        let mut tmp = [0u8; 512];
                        tmp[..p_len].copy_from_slice(&parrafo[..p_len]);
                        self.empujar_envuelto(&tmp[..p_len], tipo_actual);
                        p_len = 0;
                        if matches!(t, b"/h1" | b"/h2" | b"/h3" | b"/p") {
                            self.empujar_linea(b"", LINEA_TEXTO);
                        }
                    }
                    if matches!(t, b"h1" | b"h2" | b"h3") {
                        tipo_actual = LINEA_TITULO;
                    } else if matches!(t, b"/h1" | b"/h2" | b"/h3" | b"p" | b"/p" | b"/a") {
                        tipo_actual = LINEA_TEXTO;
                    } else if t == b"a" {
                        if p_len == 0 {
                            tipo_actual = LINEA_ENLACE;
                        }
                    } else if t == b"li" {
                        if p_len + 2 < parrafo.len() {
                            parrafo[p_len] = b'*';
                            parrafo[p_len + 1] = b' ';
                            p_len += 2;
                        }
                    }
                }
                i = (tag_fin + 1).min(fin);
                continue;
            }

            if en_script || en_style {
                i += 1;
                continue;
            }

            // Decodificar entidades basicas (&amp; &lt; &gt; &quot; &nbsp;)
            let ch = if b == b'&' {
                let mut semi = i + 1;
                while semi < fin && semi < i + 8 && self.raw_buf[semi] != b';' {
                    semi += 1;
                }
                if semi < fin && self.raw_buf[semi] == b';' {
                    let ent = &self.raw_buf[i + 1..semi];
                    let dec = match ent {
                        b"amp" => b'&',
                        b"lt" => b'<',
                        b"gt" => b'>',
                        b"quot" => b'"',
                        b"apos" | b"#39" => b'\'',
                        b"nbsp" => b' ',
                        _ => b' ',
                    };
                    i = semi + 1;
                    dec
                } else {
                    i += 1;
                    b'&'
                }
            } else {
                i += 1;
                if b == b'\r' || b == b'\n' || b == b'\t' {
                    b' '
                } else if b >= 32 && b <= 126 {
                    b
                } else {
                    b' '
                }
            };

            if en_title {
                if tit_len < tit_buf.len() && !(ch == b' ' && (tit_len == 0 || tit_buf[tit_len - 1] == b' ')) {
                    tit_buf[tit_len] = ch;
                    tit_len += 1;
                }
            } else {
                if ch == b' ' && (p_len == 0 || parrafo[p_len - 1] == b' ') {
                    continue;
                }
                if p_len < parrafo.len() {
                    parrafo[p_len] = ch;
                    p_len += 1;
                } else {
                    let mut tmp = [0u8; 512];
                    tmp.copy_from_slice(&parrafo);
                    self.empujar_envuelto(&tmp, tipo_actual);
                    p_len = 0;
                }
            }
        }

        if p_len > 0 {
            let mut tmp = [0u8; 512];
            tmp[..p_len].copy_from_slice(&parrafo[..p_len]);
            self.empujar_envuelto(&tmp[..p_len], tipo_actual);
        }
    }

    pub fn dibujar(
        &self,
        l: &mut Lienzo,
        vx: i32,
        vy: i32,
        vw: i32,
        vh: i32,
        acento: u32,
        borde_col: u32,
        txt: u32,
        txt2: u32,
    ) {
        let bx = vx + 18;
        let bw = vw - 36;
        let by = vy + 42;

        // Barra de direcciones + boton [Ir]
        let url_w = bw - 82;
        l.rect_redondo(bx, by, url_w, 30, 10, 0x0A0D14, 210);
        l.borde_redondo(
            bx,
            by,
            url_w,
            30,
            10,
            if self.editando_url { acento } else { borde_col },
            if self.editando_url { 230 } else { 42 },
        );
        l.texto_ui_lim(bx + 12, by + 6, &self.url[..self.url_len], url_w - 24, txt);
        if self.editando_url {
            let cx = bx + 12 + Lienzo::ancho_ui(&self.url[..self.url_len]).min(url_w - 26);
            l.rect_redondo(cx + 1, by + 6, 2, 17, 1, acento, 255);
        }

        l.rect_redondo(bx + url_w + 8, by, 74, 30, 10, acento, 235);
        l.texto_ui_centrado(bx + url_w + 45, by + 6, b"Ir", 0xFFFFFF);

        // Barra de marcadores rapidos (1 clic -> Internet real!)
        let my = by + 38;
        let marks: [&[u8]; 4] = [b"Inicio", b"example.com", b"info.cern.ch", b"neverssl.com"];
        for (i, &m) in marks.iter().enumerate() {
            let mx = bx + (i as i32) * 124;
            l.rect_redondo(mx, my, 116, 24, 8, 0x1D2534, 210);
            l.borde_redondo(mx, my, 116, 24, 8, borde_col, 32);
            l.texto_ui_centrado(mx + 58, my + 3, m, 0x38BDF8);
        }
        l.texto_ui_lim(bx + 506, my + 3, &self.estado[..self.estado_len], bw - 510, txt2);

        // Area de contenido de la pagina
        let cy0 = my + 32;
        let ch = vh - (cy0 - vy) - 18;
        l.rect_redondo(bx, cy0, bw, ch, 12, 0x0A0D14, 190);
        l.borde_redondo(bx, cy0, bw, ch, 12, borde_col, 34);

        // Encabezado de pestana con titulo de la pagina
        l.rect_alfa(bx + 12, cy0 + 30, bw - 24, 1, borde_col, 24);
        l.texto_ui_lim(bx + 16, cy0 + 7, &self.titulo[..self.titulo_len], bw - 32, acento);

        let mut y = cy0 + 40;
        let max_y = cy0 + ch - 22;
        for idx in self.scroll..self.num_lineas {
            if y > max_y {
                break;
            }
            let s = &self.lineas[idx][..self.lineas_len[idx]];
            match self.lineas_tipo[idx] {
                LINEA_TITULO => {
                    l.texto_ui_lim(bx + 18, y, s, bw - 36, 0xF8FAFC);
                    y += 24;
                }
                LINEA_ENLACE => {
                    l.texto_ui_lim(bx + 18, y, s, bw - 36, 0x38BDF8);
                    y += 20;
                }
                LINEA_META => {
                    l.texto_mono(bx + 18, y, s, 84, 0x94A3B8);
                    y += 19;
                }
                _ => {
                    l.texto_ui_lim(bx + 18, y, s, bw - 36, txt2);
                    y += 20;
                }
            }
        }
    }
}
