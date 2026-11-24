// Zweck:    Entwurfswerkzeug für die Knopfgrafiken: erzeugt je Knopf drei Vorschläge (icons/entwuerfe/NN_V.png) und
//           eine Übersichtsgrafik mit Nummern (icons/entwuerfe/Uebersicht.png). Aufruf:
//           cargo run --release --example icon_entwuerfe
// Hardware: keine
// Autor:    Johann Gerner

use image::{RgbaImage, imageops};

const SS: usize = 4;
const N: usize = 256;
const W: usize = N * SS;
type Col = [f32; 4];

fn c(r: u8, g: u8, b: u8) -> Col {
    [f32::from(r), f32::from(g), f32::from(b), 1.0]
}
fn al(col: Col, a: f32) -> Col {
    [col[0], col[1], col[2], col[3] * a]
}

/// Farbpalette eines Entwurfs.
#[derive(Clone, Copy)]
struct Pal {
    bg: Col,
    fg: Col,
    acc: Col,
    soft: Col,
    rand: Option<Col>,
}

fn palette(i: usize) -> Pal {
    match i % 3 {
        // wie „Freie Stationierung“: dunkles Petrol, weiß, orange
        0 => Pal { bg: c(20, 60, 72), fg: c(255, 255, 255), acc: c(240, 150, 40), soft: c(125, 211, 196), rand: None },
        // wie „Polarpunkte“: Blau, Papierweiß, orange
        1 => Pal { bg: c(43, 90, 142), fg: c(244, 242, 236), acc: c(242, 128, 36), soft: c(176, 199, 226), rand: None },
        // hell: cremefarbene Kachel, dunkles Petrol, orange
        _ => Pal { bg: c(240, 241, 233), fg: c(20, 60, 72), acc: c(232, 112, 24), soft: c(96, 168, 164), rand: Some(c(178, 190, 186)) },
    }
}

struct Bild {
    px: Vec<[f32; 4]>,
}

fn seg_d(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let l2 = dx * dx + dy * dy;
    let t = if l2 == 0.0 { 0.0 } else { (((px - ax) * dx + (py - ay) * dy) / l2).clamp(0.0, 1.0) };
    (px - ax - t * dx).hypot(py - ay - t * dy)
}

fn sd_rrect(px: f32, py: f32, x: f32, y: f32, w: f32, h: f32, r: f32) -> f32 {
    let qx = (px - x - w / 2.0).abs() - (w / 2.0 - r);
    let qy = (py - y - h / 2.0).abs() - (h / 2.0 - r);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

fn pip(pts: &[(f32, f32)], px: f32, py: f32) -> bool {
    let mut innen = false;
    let mut j = pts.len() - 1;
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[j]);
        if (a.1 > py) != (b.1 > py) && px < (b.0 - a.0) * (py - a.1) / (b.1 - a.1) + a.0 {
            innen = !innen;
        }
        j = i;
    }
    innen
}

impl Bild {
    fn neu() -> Self {
        Bild { px: vec![[0.0; 4]; W * W] }
    }

    fn malen(&mut self, bb: (f32, f32, f32, f32), col: Col, innen: impl Fn(f32, f32) -> bool) {
        let s = SS as f32;
        let x0 = (bb.0 * s).floor().max(0.0) as usize;
        let y0 = (bb.1 * s).floor().max(0.0) as usize;
        let x1 = ((bb.2 * s).ceil().max(0.0) as usize).min(W);
        let y1 = ((bb.3 * s).ceil().max(0.0) as usize).min(W);
        for y in y0..y1 {
            for x in x0..x1 {
                if !innen((x as f32 + 0.5) / s, (y as f32 + 0.5) / s) {
                    continue;
                }
                let d = &mut self.px[y * W + x];
                let (sa, da) = (col[3], d[3]);
                let oa = sa + da * (1.0 - sa);
                if oa > 0.0 {
                    for i in 0..3 {
                        d[i] = (col[i] * sa + d[i] * da * (1.0 - sa)) / oa;
                    }
                    d[3] = oa;
                }
            }
        }
    }

    fn rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, col: Col) {
        self.malen((x, y, x + w, y + h), col, |px, py| sd_rrect(px, py, x, y, w, h, r) <= 0.0);
    }
    fn rrect_rand(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, sw: f32, col: Col) {
        let m = sw;
        self.malen((x - m, y - m, x + w + m, y + h + m), col, |px, py| sd_rrect(px, py, x, y, w, h, r).abs() <= sw / 2.0);
    }
    fn kreis(&mut self, cx: f32, cy: f32, r: f32, col: Col) {
        self.malen((cx - r, cy - r, cx + r, cy + r), col, |px, py| (px - cx).hypot(py - cy) <= r);
    }
    fn ring(&mut self, cx: f32, cy: f32, r: f32, sw: f32, col: Col) {
        let m = r + sw;
        self.malen((cx - m, cy - m, cx + m, cy + m), col, |px, py| ((px - cx).hypot(py - cy) - r).abs() <= sw / 2.0);
    }
    fn linie(&mut self, ax: f32, ay: f32, bx: f32, by: f32, sw: f32, col: Col) {
        let m = sw / 2.0 + 1.0;
        self.malen((ax.min(bx) - m, ay.min(by) - m, ax.max(bx) + m, ay.max(by) + m), col, |px, py| seg_d(px, py, ax, ay, bx, by) <= sw / 2.0);
    }
    fn gestrichelt(&mut self, ax: f32, ay: f32, bx: f32, by: f32, sw: f32, col: Col, strich: f32, luecke: f32) {
        let l = (bx - ax).hypot(by - ay);
        let (dx, dy) = ((bx - ax) / l, (by - ay) / l);
        let mut t = 0.0;
        while t < l {
            let e = (t + strich).min(l);
            self.linie(ax + dx * t, ay + dy * t, ax + dx * e, ay + dy * e, sw, col);
            t += strich + luecke;
        }
    }
    fn vieleck(&mut self, pts: &[(f32, f32)], col: Col) {
        let bb = pts.iter().fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |b, p| (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1)));
        let pts = pts.to_vec();
        self.malen(bb, col, move |px, py| pip(&pts, px, py));
    }
    fn zug(&mut self, pts: &[(f32, f32)], geschlossen: bool, sw: f32, col: Col) {
        let n = pts.len();
        let bb = pts.iter().fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |b, p| (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1)));
        let m = sw / 2.0 + 1.0;
        let pts = pts.to_vec();
        let kanten = if geschlossen { n } else { n - 1 };
        self.malen((bb.0 - m, bb.1 - m, bb.2 + m, bb.3 + m), col, move |px, py| {
            (0..kanten).any(|i| {
                let (a, b) = (pts[i], pts[(i + 1) % n]);
                seg_d(px, py, a.0, a.1, b.0, b.1) <= sw / 2.0
            })
        });
    }
    /// Kreisbogen (Grad, im Uhrzeigersinn, 0° = rechts).
    fn bogen(&mut self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32, sw: f32, col: Col) {
        let n = ((a1 - a0).abs() / 4.0).ceil().max(2.0) as usize;
        let pts: Vec<(f32, f32)> = (0..=n)
            .map(|i| {
                let a = (a0 + (a1 - a0) * i as f32 / n as f32).to_radians();
                (cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        self.zug(&pts, false, sw, col);
    }
    /// Dreieck (Messpunkt) als Umriss.
    fn dreieck(&mut self, cx: f32, cy: f32, s: f32, sw: f32, col: Col) {
        self.zug(&[(cx, cy - s * 0.95), (cx + s, cy + s * 0.7), (cx - s, cy + s * 0.7)], true, sw, col);
    }
    fn pfeil(&mut self, ax: f32, ay: f32, bx: f32, by: f32, sw: f32, hs: f32, col: Col) {
        let l = (bx - ax).hypot(by - ay);
        let (dx, dy) = ((bx - ax) / l, (by - ay) / l);
        let (nx, ny) = (-dy, dx);
        let (kx, ky) = (bx - dx * hs, by - dy * hs);
        self.linie(ax, ay, kx + dx * 2.0, ky + dy * 2.0, sw, col);
        self.vieleck(&[(bx, by), (kx + nx * hs * 0.7, ky + ny * hs * 0.7), (kx - nx * hs * 0.7, ky - ny * hs * 0.7)], col);
    }
    fn pixeltext(&mut self, x: f32, y: f32, p: f32, t: &str, col: Col) {
        for (n, ch) in t.chars().enumerate() {
            for (zeile, bits) in glyphe(ch).iter().enumerate() {
                for spalte in 0..5 {
                    if bits >> (4 - spalte) & 1 == 1 {
                        self.rrect(x + (n * 6 + spalte) as f32 * p, y + zeile as f32 * p, p, p, p * 0.18, col);
                    }
                }
            }
        }
    }
    /// Blatt mit geknickter Ecke.
    fn blatt(&mut self, pa: &Pal, x: f32, y: f32, w: f32, h: f32) {
        let k = w * 0.28;
        self.vieleck(&[(x, y), (x + w - k, y), (x + w, y + k), (x + w, y + h), (x, y + h)], pa.fg);
        self.vieleck(&[(x + w - k, y), (x + w - k, y + k), (x + w, y + k)], pa.soft);
    }
    /// Stift: Spitze bei (tx, ty), Richtung nach rechts oben.
    fn stift(&mut self, pa: &Pal, tx: f32, ty: f32, s: f32) {
        let (ca, sa) = ((-45f32).to_radians().cos(), (-45f32).to_radians().sin());
        let f = |pts: &[(f32, f32)]| -> Vec<(f32, f32)> { pts.iter().map(|p| (tx + (p.0 * ca - p.1 * sa) * s, ty + (p.0 * sa + p.1 * ca) * s)).collect() };
        self.vieleck(&f(&[(0.0, 0.0), (34.0, -15.0), (112.0, -15.0), (112.0, 15.0), (34.0, 15.0)]), pa.fg);
        self.vieleck(&f(&[(0.0, 0.0), (22.0, -10.0), (22.0, 10.0)]), pa.acc);
        self.vieleck(&f(&[(34.0, -15.0), (46.0, -15.0), (46.0, 15.0), (34.0, 15.0)]), pa.soft);
        self.vieleck(&f(&[(112.0, -15.0), (136.0, -15.0), (136.0, 15.0), (112.0, 15.0)]), pa.acc);
    }
    /// Geschweifte Klammer links (spiegeln: x → 256 − x).
    fn klammer(&mut self, x0: f32, y0: f32, h: f32, rechts: bool, sw: f32, col: Col) {
        let m = h / 2.0;
        let pts = [(x0 + 22.0, y0), (x0 + 8.0, y0 + 10.0), (x0 + 8.0, y0 + m - 14.0), (x0 - 8.0, y0 + m), (x0 + 8.0, y0 + m + 14.0), (x0 + 8.0, y0 + h - 10.0), (x0 + 22.0, y0 + h)];
        let pts: Vec<(f32, f32)> = pts.iter().map(|p| if rechts { (256.0 - p.0, p.1) } else { *p }).collect();
        self.zug(&pts, false, sw, col);
    }
    fn kachel(&mut self, pa: &Pal) {
        self.rrect(0.0, 0.0, 256.0, 256.0, 56.0, pa.bg);
        if let Some(r) = pa.rand {
            self.rrect_rand(2.0, 2.0, 252.0, 252.0, 54.0, 4.0, r);
        }
    }
    fn herunterrechnen(&self) -> RgbaImage {
        let mut aus = RgbaImage::new(N as u32, N as u32);
        let n = (SS * SS) as f32;
        for y in 0..N {
            for x in 0..N {
                let (mut r, mut g, mut b, mut a) = (0.0, 0.0, 0.0, 0.0);
                for dy in 0..SS {
                    for dx in 0..SS {
                        let p = self.px[(y * SS + dy) * W + x * SS + dx];
                        r += p[0] * p[3];
                        g += p[1] * p[3];
                        b += p[2] * p[3];
                        a += p[3];
                    }
                }
                let (alpha, f) = (a / n, if a > 0.0 { 1.0 / a } else { 0.0 });
                aus.put_pixel(x as u32, y as u32, image::Rgba([(r * f).round() as u8, (g * f).round() as u8, (b * f).round() as u8, (alpha * 255.0).round() as u8]));
            }
        }
        aus
    }
}

/// Zeilenmuster (5 Bit breit, 7 Zeilen) eines Zeichens; unbekannte Zeichen bleiben leer.
fn glyphe(c: char) -> [u8; 7] {
    match c.to_ascii_uppercase() {
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 0b01100, 0b01100],
        ',' => [0, 0, 0, 0, 0b01100, 0b00100, 0b01000],
        '_' => [0, 0, 0, 0, 0, 0, 0b11111],
        '/' => [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000],
        ':' => [0, 0b01100, 0b01100, 0, 0b01100, 0b01100, 0],
        '=' => [0, 0b11111, 0, 0b11111, 0, 0, 0],
        '+' => [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0],
        _ => [0; 7],
    }
}



/// Knopfname (ASCII-Großschrift für die Übersicht).
const NAMEN: [&str; 26] = [
    "DXF OEFFNEN",
    "KOR/CSV IMPORT",
    "JSON IMPORT",
    "ZOOM +",
    "ZOOM -",
    "EINPASSEN",
    "PUNKTFANG",
    "KATASTERPUNKTE",
    "DXF EIN/AUS",
    "DXF LEEREN",
    "PUNKTE LOESCHEN",
    "INDEX ZURUECKSETZEN",
    "DIGITALISIERUNG",
    "AUFNAHMETEST",
    "POLARPUNKTE",
    "FESTPUNKTE UEBERNEHMEN",
    "FLAECHENBERECHNUNG",
    "PRISMENKONSTANTE",
    "EDM REFLEKTOR",
    "EDM REFLEKTORLOS",
    "LASERPOINTER",
    "PDF-AUSSCHNITT",
    "DXF-EXPORT",
    "NEUER STANDPUNKT",
    "KONTROLLE",
    "ABSTECKEN",
];

fn pfeil_runter(b: &mut Bild, p: &Pal, x: f32, y0: f32, y1: f32) {
    b.pfeil(x, y0, x, y1, 14.0, 30.0, p.acc);
}

fn zeichne(b: &mut Bild, p: &Pal, id: usize, v: usize) {
    let (fg, acc, soft, bg) = (p.fg, p.acc, p.soft, p.bg);
    match (id, v) {
        // ── DXF öffnen ──
        (0, 0) => {
            b.rrect(46.0, 70.0, 72.0, 44.0, 12.0, soft);
            b.rrect(46.0, 90.0, 164.0, 110.0, 14.0, soft);
            b.vieleck(&[(40.0, 126.0), (218.0, 126.0), (196.0, 204.0), (62.0, 204.0)], fg);
            b.kreis(176.0, 90.0, 12.0, acc);
        }
        (0, 1) => {
            b.blatt(p, 68.0, 44.0, 120.0, 168.0);
            b.pixeltext(77.0, 108.0, 6.0, "DXF", bg);
            b.linie(88.0, 168.0, 168.0, 168.0, 10.0, acc);
        }
        (0, 2) => {
            b.zug(&[(44.0, 204.0), (44.0, 72.0), (104.0, 72.0), (122.0, 94.0), (212.0, 94.0), (212.0, 204.0)], true, 14.0, fg);
            b.dreieck(128.0, 152.0, 28.0, 10.0, acc);
        }
        // ── KOR/CSV Import ──
        (1, 0) => {
            b.blatt(p, 66.0, 84.0, 124.0, 140.0);
            b.pixeltext(86.0, 140.0, 5.0, "KOR", bg);
            pfeil_runter(b, p, 128.0, 20.0, 78.0);
        }
        (1, 1) => {
            b.rrect_rand(52.0, 90.0, 152.0, 128.0, 14.0, 10.0, fg);
            for x in [103.0, 154.0] {
                b.linie(x, 90.0, x, 218.0, 8.0, fg);
            }
            for y in [133.0, 176.0] {
                b.linie(52.0, y, 204.0, y, 8.0, fg);
            }
            for (x, y) in [(78.0, 112.0), (129.0, 155.0), (180.0, 197.0)] {
                b.kreis(x, y, 11.0, acc);
            }
            pfeil_runter(b, p, 128.0, 18.0, 76.0);
        }
        (1, 2) => {
            b.zug(&[(52.0, 156.0), (52.0, 212.0), (204.0, 212.0), (204.0, 156.0)], false, 14.0, fg);
            b.pfeil(128.0, 28.0, 128.0, 180.0, 16.0, 38.0, acc);
            b.dreieck(66.0, 80.0, 18.0, 8.0, soft);
            b.dreieck(190.0, 80.0, 18.0, 8.0, soft);
        }
        // ── JSON Import ──
        (2, 0) => {
            b.blatt(p, 66.0, 84.0, 124.0, 140.0);
            b.pixeltext(80.0, 140.0, 4.5, "JSON", bg);
            pfeil_runter(b, p, 128.0, 20.0, 78.0);
        }
        (2, 1) => {
            b.klammer(70.0, 92.0, 124.0, false, 14.0, fg);
            b.klammer(70.0, 92.0, 124.0, true, 14.0, fg);
            for x in [108.0, 128.0, 148.0] {
                b.kreis(x, 154.0, 8.0, acc);
            }
            pfeil_runter(b, p, 128.0, 16.0, 74.0);
        }
        (2, 2) => {
            b.zug(&[(52.0, 156.0), (52.0, 212.0), (204.0, 212.0), (204.0, 156.0)], false, 14.0, fg);
            b.klammer(58.0, 40.0, 100.0, false, 12.0, soft);
            b.klammer(58.0, 40.0, 100.0, true, 12.0, soft);
            b.pfeil(128.0, 28.0, 128.0, 180.0, 16.0, 38.0, acc);
        }
        // ── Zoom + / − ──
        (3 | 4, 0) => {
            b.ring(112.0, 112.0, 62.0, 16.0, fg);
            b.linie(158.0, 158.0, 212.0, 212.0, 24.0, fg);
            b.linie(84.0, 112.0, 140.0, 112.0, 13.0, acc);
            if id == 3 {
                b.linie(112.0, 84.0, 112.0, 140.0, 13.0, acc);
            }
        }
        (3 | 4, 1) => {
            b.kreis(112.0, 112.0, 62.0, al(soft, 0.45));
            b.ring(112.0, 112.0, 62.0, 14.0, fg);
            b.linie(158.0, 158.0, 210.0, 210.0, 28.0, acc);
            b.linie(86.0, 112.0, 138.0, 112.0, 13.0, fg);
            if id == 3 {
                b.linie(112.0, 86.0, 112.0, 138.0, 13.0, fg);
            }
        }
        (3 | 4, 2) => {
            for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
                let (x, y) = (128.0 - 92.0 * sx, 128.0 - 92.0 * sy);
                b.zug(&[(x + 40.0 * sx, y), (x, y), (x, y + 40.0 * sy)], false, 14.0, fg);
            }
            b.ring(118.0, 118.0, 34.0, 12.0, acc);
            b.linie(142.0, 142.0, 176.0, 176.0, 16.0, acc);
            b.linie(104.0, 118.0, 132.0, 118.0, 10.0, fg);
            if id == 3 {
                b.linie(118.0, 104.0, 118.0, 132.0, 10.0, fg);
            }
        }
        // ── Einpassen ──
        (5, 0) => {
            for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
                let (x, y) = (128.0 - 86.0 * sx, 128.0 - 86.0 * sy);
                b.zug(&[(x + 46.0 * sx, y), (x, y), (x, y + 46.0 * sy)], false, 16.0, fg);
            }
            b.rrect(98.0, 98.0, 60.0, 60.0, 10.0, acc);
        }
        (5, 1) => {
            for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
                b.pfeil(128.0 + 22.0 * sx, 128.0 + 22.0 * sy, 128.0 + 84.0 * sx, 128.0 + 84.0 * sy, 12.0, 28.0, fg);
            }
            b.ring(128.0, 128.0, 14.0, 9.0, acc);
        }
        (5, 2) => {
            b.rrect_rand(40.0, 56.0, 176.0, 144.0, 14.0, 12.0, fg);
            b.zug(&[(86.0, 166.0), (100.0, 100.0), (152.0, 92.0), (176.0, 150.0)], true, 10.0, acc);
            for (x, y) in [(86.0, 166.0), (100.0, 100.0), (152.0, 92.0), (176.0, 150.0)] {
                b.kreis(x, y, 9.0, fg);
            }
        }
        // ── Punktfang ──
        (6, 0) => {
            b.rrect_rand(62.0, 62.0, 132.0, 132.0, 8.0, 12.0, acc);
            for (ax, ay, bx, by) in [(36.0, 128.0, 62.0, 128.0), (194.0, 128.0, 220.0, 128.0), (128.0, 36.0, 128.0, 62.0), (128.0, 194.0, 128.0, 220.0)] {
                b.linie(ax, ay, bx, by, 12.0, acc);
            }
            b.dreieck(128.0, 130.0, 26.0, 10.0, fg);
        }
        (6, 1) => {
            b.bogen(128.0, 118.0, 56.0, 180.0, 360.0, 28.0, acc);
            for x in [58.0, 170.0] {
                b.rrect(x, 118.0, 28.0, 58.0, 6.0, acc);
                b.rrect(x, 176.0, 28.0, 28.0, 6.0, fg);
            }
            b.kreis(128.0, 222.0, 9.0, soft);
        }
        (6, 2) => {
            b.dreieck(104.0, 118.0, 44.0, 14.0, fg);
            b.vieleck(&[(138.0, 124.0), (138.0, 214.0), (160.0, 192.0), (178.0, 230.0), (196.0, 222.0), (178.0, 186.0), (208.0, 184.0)], acc);
        }
        // ── Katasterpunkte ──
        (7, 0) => {
            b.gestrichelt(128.0, 76.0, 76.0, 168.0, 8.0, soft, 14.0, 10.0);
            b.gestrichelt(128.0, 76.0, 184.0, 168.0, 8.0, soft, 14.0, 10.0);
            b.gestrichelt(76.0, 176.0, 184.0, 176.0, 8.0, soft, 14.0, 10.0);
            for (x, y) in [(128.0, 72.0), (70.0, 174.0), (186.0, 174.0)] {
                b.dreieck(x, y, 28.0, 12.0, fg);
            }
        }
        (7, 1) => {
            b.kreis(128.0, 100.0, 54.0, acc);
            b.vieleck(&[(84.0, 130.0), (172.0, 130.0), (128.0, 218.0)], acc);
            b.kreis(128.0, 100.0, 22.0, bg);
        }
        (7, 2) => {
            for gx in 0..3 {
                for gy in 0..3 {
                    let (x, y) = (68.0 + 60.0 * gx as f32, 68.0 + 60.0 * gy as f32);
                    if gx == 1 && gy == 1 {
                        b.ring(x, y, 20.0, 10.0, acc);
                    } else {
                        b.kreis(x, y, 12.0, fg);
                    }
                }
            }
        }
        // ── DXF-Darstellung ein/aus ──
        (8, 0) => {
            let mut pts: Vec<(f32, f32)> = (0..=20).map(|i| {
                let t = i as f32 / 20.0 * std::f32::consts::PI;
                (36.0 + 184.0 * (i as f32 / 20.0), 128.0 - 54.0 * t.sin())
            }).collect();
            pts.extend((0..=20).rev().map(|i| {
                let t = i as f32 / 20.0 * std::f32::consts::PI;
                (36.0 + 184.0 * (i as f32 / 20.0), 128.0 + 54.0 * t.sin())
            }));
            b.zug(&pts, true, 14.0, fg);
            b.ring(128.0, 128.0, 27.0, 14.0, acc);
            b.kreis(128.0, 128.0, 9.0, acc);
        }
        (8, 1) => {
            for (dy, col) in [(60.0, al(fg, 0.4)), (30.0, soft), (0.0, acc)] {
                b.vieleck(&[(128.0, 52.0 + dy), (214.0, 94.0 + dy), (128.0, 136.0 + dy), (42.0, 94.0 + dy)], col);
            }
            b.zug(&[(128.0, 52.0), (214.0, 94.0), (128.0, 136.0), (42.0, 94.0)], true, 7.0, fg);
        }
        (8, 2) => {
            b.zug(&[(58.0, 118.0), (80.0, 52.0), (150.0, 40.0), (198.0, 96.0), (140.0, 132.0)], true, 10.0, acc);
            for (x, y) in [(58.0, 118.0), (80.0, 52.0), (150.0, 40.0), (198.0, 96.0), (140.0, 132.0)] {
                b.kreis(x, y, 10.0, fg);
            }
            b.rrect(60.0, 170.0, 136.0, 56.0, 28.0, soft);
            b.kreis(168.0, 198.0, 22.0, fg);
        }
        // ── DXF leeren ──
        (9, 0) => {
            b.linie(62.0, 84.0, 194.0, 84.0, 14.0, fg);
            b.linie(104.0, 66.0, 152.0, 66.0, 14.0, fg);
            b.vieleck(&[(76.0, 102.0), (180.0, 102.0), (168.0, 212.0), (88.0, 212.0)], fg);
            for x in [108.0, 128.0, 148.0] {
                b.linie(x, 122.0, x, 192.0, 8.0, bg);
            }
        }
        (9, 1) => {
            b.blatt(p, 68.0, 42.0, 120.0, 172.0);
            b.linie(98.0, 112.0, 158.0, 172.0, 18.0, acc);
            b.linie(158.0, 112.0, 98.0, 172.0, 18.0, acc);
        }
        (9, 2) => {
            for (ax, ay, bx, by) in [(56.0, 56.0, 200.0, 56.0), (200.0, 56.0, 200.0, 200.0), (200.0, 200.0, 56.0, 200.0), (56.0, 200.0, 56.0, 56.0)] {
                b.gestrichelt(ax, ay, bx, by, 10.0, fg, 22.0, 14.0);
            }
            b.linie(104.0, 104.0, 152.0, 152.0, 14.0, acc);
            b.linie(152.0, 104.0, 104.0, 152.0, 14.0, acc);
        }
        // ── Punkte löschen ──
        (10, 0) => {
            for (ax, ay, bx, by) in [(48.0, 60.0, 208.0, 60.0), (208.0, 60.0, 208.0, 196.0), (208.0, 196.0, 48.0, 196.0), (48.0, 196.0, 48.0, 60.0)] {
                b.gestrichelt(ax, ay, bx, by, 9.0, acc, 20.0, 12.0);
            }
            for (x, y) in [(86.0, 104.0), (146.0, 96.0), (112.0, 156.0)] {
                b.kreis(x, y, 12.0, fg);
            }
            b.linie(158.0, 138.0, 200.0, 180.0, 14.0, acc);
            b.linie(200.0, 138.0, 158.0, 180.0, 14.0, acc);
        }
        (10, 1) => {
            let (ca, sa) = ((-35f32).to_radians().cos(), (-35f32).to_radians().sin());
            let f = |pts: &[(f32, f32)]| -> Vec<(f32, f32)> { pts.iter().map(|q| (128.0 + q.0 * ca - q.1 * sa, 112.0 + q.0 * sa + q.1 * ca)).collect() };
            b.vieleck(&f(&[(-72.0, -34.0), (-8.0, -34.0), (-8.0, 34.0), (-72.0, 34.0)]), acc);
            b.vieleck(&f(&[(-8.0, -34.0), (72.0, -34.0), (72.0, 34.0), (-8.0, 34.0)]), fg);
            b.gestrichelt(44.0, 206.0, 212.0, 206.0, 8.0, soft, 18.0, 12.0);
            b.kreis(90.0, 206.0, 9.0, soft);
        }
        (10, 2) => {
            b.dreieck(128.0, 128.0, 56.0, 14.0, fg);
            b.linie(62.0, 62.0, 194.0, 194.0, 20.0, acc);
            b.linie(194.0, 62.0, 62.0, 194.0, 20.0, acc);
        }
        // ── Index zurücksetzen ──
        (11, 0) | (11, 2) => {
            b.bogen(128.0, 128.0, 70.0, 60.0, 320.0, 18.0, fg);
            let (a, r) = (320f32.to_radians(), 70.0);
            let (px, py) = (128.0 + r * a.cos(), 128.0 + r * a.sin());
            let (tx, ty) = (-a.sin(), a.cos());
            let (nx, ny) = (ty, -tx);
            b.vieleck(&[(px + tx * 30.0, py + ty * 30.0), (px + nx * 26.0 - tx * 4.0, py + ny * 26.0 - ty * 4.0), (px - nx * 26.0 - tx * 4.0, py - ny * 26.0 - ty * 4.0)], fg);
            if v == 0 {
                b.pixeltext(94.0, 112.0, 4.0, "123", acc);
            } else {
                b.dreieck(128.0, 132.0, 24.0, 9.0, acc);
            }
        }
        (11, 1) => {
            for (i, t) in ["1", "2", "3"].iter().enumerate() {
                let y = 46.0 + 52.0 * i as f32;
                b.pixeltext(50.0, y, 6.0, t, fg);
                b.rrect(100.0, y + 10.0, 110.0, 22.0, 8.0, soft);
            }
            let (cx, cy, r) = (194.0f32, 204.0f32, 30.0f32);
            b.bogen(cx, cy, r, 60.0, 320.0, 11.0, acc);
            let a = 320f32.to_radians();
            let (px, py) = (cx + r * a.cos(), cy + r * a.sin());
            let (tx, ty) = (-a.sin(), a.cos());
            let (nx, ny) = (ty, -tx);
            b.vieleck(&[(px + tx * 16.0, py + ty * 16.0), (px + nx * 14.0 - tx * 2.0, py + ny * 14.0 - ty * 2.0), (px - nx * 14.0 - tx * 2.0, py - ny * 14.0 - ty * 2.0)], acc);
        }
        // ── Digitalisierung ──
        (12, 0) => {
            b.gestrichelt(70.0, 204.0, 212.0, 204.0, 8.0, soft, 18.0, 12.0);
            b.kreis(70.0, 204.0, 11.0, acc);
            b.stift(p, 78.0, 194.0, 1.0);
        }
        (12, 1) => {
            b.dreieck(184.0, 190.0, 26.0, 10.0, soft);
            b.gestrichelt(78.0, 194.0, 156.0, 194.0, 8.0, soft, 14.0, 10.0);
            b.stift(p, 78.0, 194.0, 1.0);
        }
        (12, 2) => {
            b.ring(118.0, 138.0, 56.0, 10.0, fg);
            b.linie(118.0, 50.0, 118.0, 226.0, 8.0, fg);
            b.linie(30.0, 138.0, 206.0, 138.0, 8.0, fg);
            b.kreis(118.0, 138.0, 10.0, acc);
            b.stift(p, 150.0, 102.0, 0.9);
        }
        // ── Aufnahmetest ──
        (13, 0) => {
            b.ring(128.0, 128.0, 70.0, 14.0, fg);
            b.ring(128.0, 128.0, 36.0, 12.0, fg);
            b.kreis(128.0, 128.0, 14.0, acc);
        }
        (13, 1) => {
            for x in [72.0, 128.0, 184.0] {
                b.linie(128.0, 150.0, x, 226.0, 12.0, fg);
            }
            b.rrect(88.0, 60.0, 80.0, 90.0, 18.0, fg);
            b.rrect(108.0, 36.0, 40.0, 24.0, 8.0, soft);
            b.ring(128.0, 100.0, 22.0, 10.0, acc);
        }
        (13, 2) => {
            b.ring(128.0, 128.0, 44.0, 12.0, fg);
            b.kreis(128.0, 128.0, 10.0, acc);
            for (ax, ay, bx, by) in [(128.0, 54.0, 128.0, 76.0), (128.0, 180.0, 128.0, 202.0), (54.0, 128.0, 76.0, 128.0), (180.0, 128.0, 202.0, 128.0)] {
                b.linie(ax, ay, bx, by, 10.0, fg);
            }
            b.bogen(128.0, 128.0, 90.0, 20.0, 160.0, 12.0, acc);
            b.pfeil(60.0, 176.0, 56.0, 150.0, 1.0, 1.0, al(acc, 0.0));
            b.vieleck(&[(40.0, 178.0), (76.0, 180.0), (56.0, 206.0)], acc);
        }
        // ── Polarpunkte ──
        (14, 0) => {
            for (x, y) in [(56.0, 70.0), (204.0, 84.0), (190.0, 206.0)] {
                b.gestrichelt(128.0, 128.0, x, y, 8.0, soft, 14.0, 10.0);
                b.kreis(x, y, 15.0, acc);
            }
            b.dreieck(128.0, 128.0, 28.0, 12.0, fg);
        }
        (14, 1) => {
            for r in [32.0, 64.0, 96.0] {
                b.ring(128.0, 128.0, r, 6.0, al(soft, 0.8));
            }
            for k in 0..4 {
                let a = (k as f32 * 45.0).to_radians();
                b.linie(128.0 - 100.0 * a.cos(), 128.0 - 100.0 * a.sin(), 128.0 + 100.0 * a.cos(), 128.0 + 100.0 * a.sin(), 4.0, al(soft, 0.6));
            }
            b.linie(128.0, 128.0, 196.0, 72.0, 11.0, fg);
            b.kreis(128.0, 128.0, 11.0, fg);
            b.kreis(196.0, 72.0, 16.0, acc);
        }
        (14, 2) => {
            b.rrect(56.0, 40.0, 144.0, 176.0, 14.0, fg);
            for x in [96.0, 136.0, 176.0] {
                b.linie(x, 52.0, x, 204.0, 5.0, soft);
            }
            for y in [80.0, 120.0, 160.0, 196.0] {
                b.linie(66.0, y, 190.0, y, 5.0, soft);
            }
            b.gestrichelt(100.0, 100.0, 156.0, 166.0, 7.0, bg, 12.0, 8.0);
            b.dreieck(100.0, 98.0, 22.0, 9.0, acc);
            b.kreis(156.0, 166.0, 13.0, acc);
        }
        // ── Festpunkte übernehmen ──
        (15, 0) => {
            b.ring(60.0, 116.0, 26.0, 13.0, acc);
            b.pfeil(98.0, 116.0, 148.0, 116.0, 13.0, 28.0, fg);
            b.dreieck(194.0, 114.0, 34.0, 13.0, fg);
            b.pixeltext(100.0, 172.0, 5.0, "FP", acc);
        }
        (15, 1) => {
            b.linie(96.0, 48.0, 96.0, 206.0, 12.0, fg);
            b.vieleck(&[(100.0, 54.0), (188.0, 86.0), (100.0, 118.0)], acc);
            b.dreieck(96.0, 214.0, 22.0, 9.0, soft);
            b.gestrichelt(150.0, 190.0, 210.0, 190.0, 8.0, soft, 12.0, 8.0);
        }
        (15, 2) => {
            b.pixeltext(62.0, 56.0, 12.0, "FP", fg);
            b.pfeil(56.0, 188.0, 200.0, 188.0, 14.0, 30.0, acc);
        }
        // ── Flächenberechnung ──
        (16, 0) => {
            let pts = [(56.0, 72.0), (190.0, 56.0), (210.0, 150.0), (130.0, 206.0), (62.0, 170.0)];
            b.vieleck(&pts, al(soft, 0.6));
            b.zug(&pts, true, 10.0, fg);
            for (x, y) in pts {
                b.kreis(x, y, 14.0, acc);
            }
            b.pixeltext(92.0, 106.0, 7.0, "M2", bg);
        }
        (16, 1) => {
            let pts = [(52.0, 52.0), (204.0, 52.0), (204.0, 116.0), (116.0, 116.0), (116.0, 204.0), (52.0, 204.0)];
            b.vieleck(&pts, al(acc, 0.75));
            b.zug(&pts, true, 10.0, fg);
            for (x, y) in pts {
                b.ring(x, y, 11.0, 8.0, fg);
            }
            b.gestrichelt(52.0, 232.0, 116.0, 232.0, 6.0, soft, 10.0, 6.0);
        }
        (16, 2) => {
            let pts = [(128.0, 44.0), (216.0, 196.0), (40.0, 196.0)];
            b.vieleck(&pts, al(acc, 0.85));
            b.zug(&pts, true, 12.0, fg);
            for (x, y) in pts {
                b.kreis(x, y, 13.0, fg);
            }
            b.pixeltext(113.0, 128.0, 6.0, "A", bg);
        }
        // ── Prismenkonstante ──
        (17, 0) => {
            b.zug(&[(128.0, 36.0), (216.0, 198.0), (40.0, 198.0)], true, 16.0, fg);
            for (x, y) in [(128.0, 36.0), (216.0, 198.0), (40.0, 198.0)] {
                b.linie(128.0, 144.0, x, y, 8.0, soft);
            }
            b.kreis(128.0, 144.0, 15.0, acc);
        }
        (17, 1) => {
            b.ring(128.0, 128.0, 82.0, 16.0, fg);
            let hex: Vec<(f32, f32)> = (0..6).map(|i| ((128.0 + 46.0 * (i as f32 * 60.0 + 30.0).to_radians().cos()), (128.0 + 46.0 * (i as f32 * 60.0 + 30.0).to_radians().sin()))).collect();
            b.zug(&hex, true, 8.0, soft);
            b.kreis(128.0, 128.0, 15.0, acc);
        }
        (17, 2) => {
            b.vieleck(&[(128.0, 36.0), (190.0, 142.0), (66.0, 142.0)], fg);
            b.vieleck(&[(128.0, 76.0), (160.0, 128.0), (96.0, 128.0)], acc);
            b.linie(128.0, 142.0, 128.0, 222.0, 14.0, fg);
            b.linie(92.0, 224.0, 164.0, 224.0, 12.0, fg);
        }
        // ── EDM Reflektor ──
        (18, 0) => {
            b.dreieck(184.0, 128.0, 42.0, 13.0, fg);
            b.pfeil(36.0, 108.0, 124.0, 108.0, 11.0, 26.0, acc);
            b.gestrichelt(124.0, 150.0, 40.0, 150.0, 8.0, soft, 14.0, 9.0);
        }
        (18, 1) => {
            b.ring(128.0, 128.0, 86.0, 13.0, fg);
            b.pixeltext(93.0, 79.0, 14.0, "P", acc);
        }
        (18, 2) => {
            b.dreieck(86.0, 128.0, 44.0, 13.0, fg);
            for r in [34.0, 62.0, 90.0] {
                b.bogen(126.0, 128.0, r, -42.0, 42.0, 10.0, acc);
            }
        }
        // ── EDM reflektorlos ──
        (19, 0) => {
            b.rrect(176.0, 46.0, 34.0, 164.0, 6.0, soft);
            for y in [76.0, 112.0, 148.0, 184.0] {
                b.linie(178.0, y + 26.0, 208.0, y, 5.0, bg);
            }
            b.pfeil(36.0, 128.0, 168.0, 128.0, 12.0, 28.0, acc);
        }
        (19, 1) => {
            b.pixeltext(62.0, 62.0, 12.0, "RL", fg);
            b.gestrichelt(44.0, 196.0, 212.0, 196.0, 12.0, acc, 22.0, 14.0);
        }
        (19, 2) => {
            b.dreieck(70.0, 96.0, 28.0, 10.0, fg);
            b.rrect(40.0, 190.0, 176.0, 28.0, 8.0, soft);
            b.gestrichelt(78.0, 110.0, 164.0, 182.0, 9.0, acc, 16.0, 10.0);
            b.ring(170.0, 186.0, 13.0, 8.0, acc);
        }
        // ── Laserpointer ──
        (20, 0) => {
            b.kreis(128.0, 128.0, 30.0, acc);
            for k in 0..8 {
                let a = (k as f32 * 45.0 + 22.5).to_radians();
                b.linie(128.0 + 54.0 * a.cos(), 128.0 + 54.0 * a.sin(), 128.0 + 92.0 * a.cos(), 128.0 + 92.0 * a.sin(), 12.0, fg);
            }
        }
        (20, 1) => {
            let (ca, sa) = ((-45f32).to_radians().cos(), (-45f32).to_radians().sin());
            let f = |pts: &[(f32, f32)]| -> Vec<(f32, f32)> { pts.iter().map(|q| (82.0 + q.0 * ca - q.1 * sa, 178.0 + q.0 * sa + q.1 * ca)).collect() };
            b.vieleck(&f(&[(0.0, -20.0), (110.0, -20.0), (110.0, 20.0), (0.0, 20.0)]), fg);
            b.vieleck(&f(&[(110.0, -20.0), (130.0, -12.0), (130.0, 12.0), (110.0, 20.0)]), soft);
            b.kreis(100.0, 160.0, 9.0, acc);
            b.linie(160.0, 100.0, 214.0, 46.0, 9.0, acc);
            b.kreis(218.0, 42.0, 14.0, acc);
        }
        (20, 2) => {
            b.dreieck(60.0, 196.0, 26.0, 10.0, fg);
            b.linie(80.0, 178.0, 176.0, 84.0, 9.0, acc);
            b.ring(190.0, 70.0, 22.0, 9.0, fg);
            b.kreis(190.0, 70.0, 8.0, acc);
        }
        // ── PDF-Ausschnitt ──
        (21, 0) => {
            b.blatt(p, 66.0, 44.0, 124.0, 168.0);
            b.pixeltext(77.0, 108.0, 6.0, "PDF", bg);
            b.linie(88.0, 168.0, 168.0, 168.0, 10.0, acc);
        }
        (21, 1) => {
            b.rrect(48.0, 100.0, 160.0, 82.0, 16.0, fg);
            b.rrect(82.0, 48.0, 92.0, 66.0, 6.0, soft);
            b.rrect(90.0, 150.0, 76.0, 64.0, 6.0, acc);
            b.kreis(182.0, 124.0, 7.0, bg);
        }
        (21, 2) => {
            for (ax, ay, bx, by) in [(50.0, 66.0, 206.0, 66.0), (206.0, 66.0, 206.0, 190.0), (206.0, 190.0, 50.0, 190.0), (50.0, 190.0, 50.0, 66.0)] {
                b.gestrichelt(ax, ay, bx, by, 10.0, acc, 22.0, 14.0);
            }
            b.pixeltext(69.0, 104.0, 7.0, "PDF", fg);
        }
        // ── DXF-Export ──
        (22, 0) => {
            b.blatt(p, 52.0, 62.0, 112.0, 150.0);
            b.pixeltext(64.0, 134.0, 5.0, "DXF", bg);
            b.pfeil(132.0, 116.0, 210.0, 44.0, 14.0, 30.0, acc);
        }
        (22, 1) => {
            let pts = [(44.0, 160.0), (62.0, 84.0), (116.0, 62.0), (140.0, 124.0), (98.0, 176.0)];
            b.zug(&pts, true, 11.0, fg);
            for (x, y) in pts {
                b.kreis(x, y, 9.0, soft);
            }
            b.pfeil(150.0, 190.0, 224.0, 190.0, 14.0, 30.0, acc);
        }
        (22, 2) => {
            b.blatt(p, 90.0, 36.0, 112.0, 150.0);
            b.rrect(60.0, 64.0, 112.0, 150.0, 4.0, al(bg, 0.0));
            b.blatt(p, 56.0, 74.0, 112.0, 150.0);
            b.pixeltext(68.0, 148.0, 5.0, "DXF", bg);
            b.kreis(196.0, 200.0, 20.0, acc);
        }
        // ── Neuer Standpunkt ──
        (23, 0) => {
            b.dreieck(108.0, 148.0, 54.0, 15.0, fg);
            b.kreis(188.0, 76.0, 38.0, acc);
            b.linie(188.0, 56.0, 188.0, 96.0, 12.0, bg);
            b.linie(168.0, 76.0, 208.0, 76.0, 12.0, bg);
        }
        (23, 1) => {
            for x in [64.0, 106.0, 148.0] {
                b.linie(106.0, 136.0, x, 218.0, 11.0, fg);
            }
            b.rrect(74.0, 52.0, 64.0, 84.0, 16.0, fg);
            b.ring(106.0, 90.0, 17.0, 9.0, acc);
            b.kreis(194.0, 66.0, 32.0, acc);
            b.linie(194.0, 48.0, 194.0, 84.0, 11.0, bg);
            b.linie(176.0, 66.0, 212.0, 66.0, 11.0, bg);
        }
        (23, 2) => {
            b.ring(104.0, 150.0, 44.0, 14.0, acc);
            b.linie(104.0, 86.0, 104.0, 214.0, 9.0, fg);
            b.linie(40.0, 150.0, 168.0, 150.0, 9.0, fg);
            b.linie(188.0, 46.0, 188.0, 106.0, 16.0, soft);
            b.linie(158.0, 76.0, 218.0, 76.0, 16.0, soft);
        }
        // ── Kontrolle ──
        (24, 0) => {
            b.dreieck(80.0, 176.0, 34.0, 12.0, fg);
            b.gestrichelt(100.0, 160.0, 160.0, 106.0, 8.0, soft, 12.0, 8.0);
            b.ring(172.0, 94.0, 22.0, 10.0, fg);
            b.zug(&[(150.0, 188.0), (176.0, 214.0), (228.0, 150.0)], false, 20.0, acc);
        }
        (24, 1) => {
            b.ring(128.0, 128.0, 88.0, 14.0, fg);
            b.zug(&[(78.0, 130.0), (116.0, 168.0), (182.0, 86.0)], false, 24.0, acc);
        }
        (24, 2) => {
            b.ring(112.0, 112.0, 56.0, 10.0, fg);
            b.linie(112.0, 32.0, 112.0, 192.0, 8.0, fg);
            b.linie(32.0, 112.0, 192.0, 112.0, 8.0, fg);
            b.kreis(112.0, 112.0, 10.0, acc);
            b.kreis(184.0, 184.0, 40.0, acc);
            b.zug(&[(164.0, 186.0), (180.0, 202.0), (206.0, 168.0)], false, 14.0, bg);
        }
        // ── Abstecken ──
        (25, 0) => {
            b.vieleck(&[(112.0, 36.0), (144.0, 36.0), (144.0, 164.0), (128.0, 214.0), (112.0, 164.0)], fg);
            b.rrect(112.0, 36.0, 32.0, 34.0, 4.0, acc);
            b.ring(128.0, 220.0, 24.0, 8.0, soft);
        }
        (25, 1) => {
            b.ring(128.0, 150.0, 60.0, 12.0, fg);
            b.linie(128.0, 70.0, 128.0, 230.0, 8.0, fg);
            b.linie(48.0, 150.0, 208.0, 150.0, 8.0, fg);
            b.pfeil(128.0, 12.0, 128.0, 112.0, 14.0, 32.0, acc);
        }
        (25, 2) => {
            b.linie(128.0, 30.0, 128.0, 140.0, 8.0, fg);
            b.ring(128.0, 36.0, 12.0, 7.0, fg);
            b.vieleck(&[(128.0, 222.0), (100.0, 140.0), (156.0, 140.0)], acc);
            b.gestrichelt(128.0, 228.0, 128.0, 236.0, 6.0, soft, 6.0, 4.0);
            b.linie(70.0, 228.0, 186.0, 228.0, 10.0, soft);
        }
        _ => {}
    }
}

/// Pixeltext in ein fertiges Bild.
fn text_in(bild: &mut RgbaImage, x: i64, y: i64, p: i64, t: &str, col: [u8; 4]) {
    for (n, ch) in t.chars().enumerate() {
        for (zeile, bits) in glyphe(ch).iter().enumerate() {
            for spalte in 0..5 {
                if bits >> (4 - spalte) & 1 == 0 {
                    continue;
                }
                for dx in 0..p {
                    for dy in 0..p {
                        let (px, py) = (x + (n as i64 * 6 + spalte) * p + dx, y + zeile as i64 * p + dy);
                        if px >= 0 && py >= 0 && px < i64::from(bild.width()) && py < i64::from(bild.height()) {
                            bild.put_pixel(px as u32, py as u32, image::Rgba(col));
                        }
                    }
                }
            }
        }
    }
}

fn kopieren(ziel: &mut RgbaImage, quelle: &RgbaImage, x0: i64, y0: i64) {
    for (x, y, p) in quelle.enumerate_pixels() {
        let a = f32::from(p.0[3]) / 255.0;
        if a == 0.0 {
            continue;
        }
        let (tx, ty) = ((x0 + i64::from(x)) as u32, (y0 + i64::from(y)) as u32);
        let alt = *ziel.get_pixel(tx, ty);
        let mix = |i: usize| (f32::from(p.0[i]) * a + f32::from(alt.0[i]) * (1.0 - a)).round() as u8;
        ziel.put_pixel(tx, ty, image::Rgba([mix(0), mix(1), mix(2), 255]));
    }
}

/// Gewählte Entwürfe: (Knopf-Nr., Entwurf, Palette (Übersteuerung), Dateiname der fertigen Grafik).
const WAHL: [(usize, usize, Option<usize>, &str); 26] = [
    (1, 2, None, "btn_dxf_oeffnen"),
    (2, 1, None, "btn_import_kor"),
    (3, 1, None, "btn_import_json"),
    (4, 2, None, "btn_zoom_plus"),
    (5, 1, None, "btn_zoom_minus"),
    (6, 2, None, "btn_einpassen"),
    (7, 2, None, "btn_punktfang"),
    (8, 2, Some(1), "btn_katasterpunkte"),
    (9, 3, None, "btn_dxf_anzeige"),
    (10, 2, None, "btn_dxf_leeren"),
    (11, 2, None, "btn_loeschen"),
    (12, 1, None, "btn_index"),
    (13, 3, None, "btn_digital"),
    (14, 3, None, "btn_aufnahmetest"),
    (15, 1, None, "btn_polarpunkte"),
    (16, 1, None, "btn_festpunkte"),
    (17, 1, None, "btn_flaeche"),
    (18, 2, None, "btn_prisma"),
    (19, 2, None, "btn_edm_prisma"),
    (20, 1, None, "btn_edm_rl"),
    (21, 1, None, "btn_laser"),
    (22, 1, None, "btn_pdf"),
    (23, 1, None, "btn_dxf_export"),
    (24, 2, None, "btn_neuer_stp"),
    (25, 1, None, "btn_kontrolle"),
    (26, 1, None, "btn_abstecken"),
];

/// Verkleinert ein Bild mit vormultipliziertem Alpha (keine dunklen Ränder an den runden Ecken).
fn verkleinern(bild: &RgbaImage, groesse: u32) -> RgbaImage {
    let mut vor: image::ImageBuffer<image::Rgba<f32>, Vec<f32>> = image::ImageBuffer::new(bild.width(), bild.height());
    for (x, y, p) in bild.enumerate_pixels() {
        let a = f32::from(p.0[3]) / 255.0;
        vor.put_pixel(x, y, image::Rgba([f32::from(p.0[0]) / 255.0 * a, f32::from(p.0[1]) / 255.0 * a, f32::from(p.0[2]) / 255.0 * a, a]));
    }
    let klein = imageops::resize(&vor, groesse, groesse, imageops::FilterType::Lanczos3);
    let mut aus = RgbaImage::new(groesse, groesse);
    for (x, y, p) in klein.enumerate_pixels() {
        let a = p.0[3].clamp(0.0, 1.0);
        let k = |i: usize| if a > 0.0 { (p.0[i] / a * 255.0).clamp(0.0, 255.0).round() as u8 } else { 0 };
        aus.put_pixel(x, y, image::Rgba([k(0), k(1), k(2), (a * 255.0).round() as u8]));
    }
    aus
}

/// Schreibt die gewählten Entwürfe als fertige Knopfgrafiken (96 × 96) nach icons/.
fn schreibe_gewaehlte() {
    for (nr, v, pal_override, name) in WAHL {
        let (id, v) = (nr - 1, v - 1);
        let pal = palette(pal_override.unwrap_or(id + v));
        let mut bild = Bild::neu();
        bild.kachel(&pal);
        zeichne(&mut bild, &pal, id, v);
        verkleinern(&bild.herunterrechnen(), 96).save(format!("icons/{name}.png")).expect("Speichern");
    }
}

fn main() {
    if std::env::args().any(|a| a == "--final") {
        schreibe_gewaehlte();
        println!("fertige Knopfgrafiken in icons/ geschrieben");
        return;
    }
    let ordner = std::path::Path::new("icons/entwuerfe");
    std::fs::create_dir_all(ordner).expect("Ordner");
    const KACHEL: u32 = 104;
    const ZEILE: i64 = 142;
    const NAME_BREITE: i64 = 300;
    const SPALTE: i64 = NAME_BREITE + 3 * 120 + 30;
    let zeilen = NAMEN.len().div_ceil(2) as i64;
    let mut uebersicht = RgbaImage::from_pixel((2 * SPALTE + 20) as u32, (zeilen * ZEILE + 70) as u32, image::Rgba([62, 66, 74, 255]));
    text_in(&mut uebersicht, 20, 18, 4, "ENTWUERFE DER KNOPFGRAFIKEN - NUMMER WAEHLEN, Z.B. 7.2", [255, 255, 255, 255]);
    for (id, name) in NAMEN.iter().enumerate() {
        let (spalte, zeile) = ((id as i64) / zeilen, (id as i64) % zeilen);
        let (x0, y0) = (10 + spalte * SPALTE, 64 + zeile * ZEILE);
        text_in(&mut uebersicht, x0 + 6, y0 + 30, 3, &format!("{}", id + 1), [255, 200, 90, 255]);
        text_in(&mut uebersicht, x0 + 6, y0 + 62, 2, name, [235, 235, 235, 255]);
        for v in 0..3 {
            let pal = palette(id + v);
            let mut bild = Bild::neu();
            bild.kachel(&pal);
            zeichne(&mut bild, &pal, id, v);
            let gross = bild.herunterrechnen();
            gross.save(ordner.join(format!("{:02}_{}.png", id + 1, v + 1))).expect("Speichern");
            let klein = imageops::resize(&gross, KACHEL, KACHEL, imageops::FilterType::Triangle);
            let tx = x0 + NAME_BREITE + v as i64 * 120;
            kopieren(&mut uebersicht, &klein, tx, y0 + 4);
            text_in(&mut uebersicht, tx + 14, y0 + 4 + i64::from(KACHEL) + 6, 3, &format!("{}.{}", id + 1, v + 1), [255, 255, 255, 255]);
        }
    }
    uebersicht.save(ordner.join("Uebersicht.png")).expect("Übersicht");
    println!("fertig: {} Knöpfe x 3 Entwürfe in {}", NAMEN.len(), ordner.display());
}
