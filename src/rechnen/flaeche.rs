// Zweck:    Flächenberechnung eines Polygons aus Eckkoordinaten (Gauß'sche Trapezformel), Umfang, Schwerpunkt,
//           Dreiecksaufteilung (zum Füllen konkaver Flächen) und Prüfung auf Selbstüberschneidung
// Hardware: keine
// Autor:    Johann Gerner

/// Punkt (R, H).
pub type Punkt = (f64, f64);

/// Kleinste Fläche [m²], ab der ein Polygon als nicht entartet gilt.
const FLAECHE_MIN: f64 = 1e-9;

/// Vorzeichenbehaftete Fläche: positiv bei Umlauf gegen den Uhrzeigersinn (R nach rechts, H nach oben).
pub fn flaeche_vorzeichen(p: &[Punkt]) -> f64 {
    if p.len() < 3 {
        return 0.0;
    }
    // Bezug auf den ersten Punkt: gute Genauigkeit auch bei Koordinaten im Millionenbereich
    let (r0, h0) = p[0];
    let summe: f64 = (0..p.len())
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            (a.0 - r0) * (b.1 - h0) - (b.0 - r0) * (a.1 - h0)
        })
        .sum();
    summe / 2.0
}

/// Fläche [m²] (immer positiv).
pub fn flaeche(p: &[Punkt]) -> f64 {
    flaeche_vorzeichen(p).abs()
}

/// Umfang [m] des geschlossenen Polygons.
pub fn umfang(p: &[Punkt]) -> f64 {
    if p.len() < 2 {
        return 0.0;
    }
    (0..p.len()).map(|i| {
        let (a, b) = (p[i], p[(i + 1) % p.len()]);
        (b.0 - a.0).hypot(b.1 - a.1)
    })
    .sum()
}

/// Flächenschwerpunkt; bei entarteter Fläche das Mittel der Ecken.
pub fn schwerpunkt(p: &[Punkt]) -> Punkt {
    if p.is_empty() {
        return (0.0, 0.0);
    }
    let (r0, h0) = p[0];
    let a = flaeche_vorzeichen(p);
    if a.abs() < FLAECHE_MIN {
        let n = p.len() as f64;
        return (p.iter().map(|q| q.0).sum::<f64>() / n, p.iter().map(|q| q.1).sum::<f64>() / n);
    }
    let (mut sr, mut sh) = (0.0, 0.0);
    for i in 0..p.len() {
        let (u, v) = ((p[i].0 - r0, p[i].1 - h0), (p[(i + 1) % p.len()].0 - r0, p[(i + 1) % p.len()].1 - h0));
        let kreuz = u.0 * v.1 - v.0 * u.1;
        sr += (u.0 + v.0) * kreuz;
        sh += (u.1 + v.1) * kreuz;
    }
    (r0 + sr / (6.0 * a), h0 + sh / (6.0 * a))
}

fn kreuz(o: Punkt, a: Punkt, b: Punkt) -> f64 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

/// Liegt `q` im (oder auf dem Rand des) Dreieck(s) `a`,`b`,`c` (gegen den Uhrzeigersinn)?
fn im_dreieck(q: Punkt, a: Punkt, b: Punkt, c: Punkt) -> bool {
    kreuz(a, b, q) >= 0.0 && kreuz(b, c, q) >= 0.0 && kreuz(c, a, q) >= 0.0
}

/// Zerlegt das Polygon in Dreiecke (Indizes in `p`), auch bei konkaven Flächen (Ear-Clipping).
/// Bei Selbstüberschneidung ist das Ergebnis nur eine Näherung, aber stets vollständig.
pub fn dreiecke(p: &[Punkt]) -> Vec<[usize; 3]> {
    let n = p.len();
    if n < 3 {
        return Vec::new();
    }
    let mut rest: Vec<usize> = (0..n).collect();
    if flaeche_vorzeichen(p) < 0.0 {
        rest.reverse();
    }
    let mut aus = Vec::with_capacity(n - 2);
    while rest.len() > 3 {
        let m = rest.len();
        let ohr = (0..m).find(|&i| {
            let (a, b, c) = (rest[(i + m - 1) % m], rest[i], rest[(i + 1) % m]);
            if kreuz(p[a], p[b], p[c]) <= 0.0 {
                return false;
            }
            !rest.iter().any(|&k| k != a && k != b && k != c && im_dreieck(p[k], p[a], p[b], p[c]))
        });
        // Kein Ohr gefunden (entartet oder selbstüberschneidend): erste Ecke abschneiden, damit es terminiert
        let i = ohr.unwrap_or(0);
        aus.push([rest[(i + m - 1) % m], rest[i], rest[(i + 1) % m]]);
        rest.remove(i);
    }
    aus.push([rest[0], rest[1], rest[2]]);
    aus
}

/// Schneiden sich zwei Strecken (echte Kreuzung, ohne bloße Berührung der Endpunkte)?
fn kreuzen(a: Punkt, b: Punkt, c: Punkt, d: Punkt) -> bool {
    let (d1, d2) = (kreuz(a, b, c), kreuz(a, b, d));
    let (d3, d4) = (kreuz(c, d, a), kreuz(c, d, b));
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

/// Überschneidet sich das Polygon selbst (Seiten kreuzen sich)? Dann ist die Fläche nicht eindeutig.
pub fn selbstschnitt(p: &[Punkt]) -> bool {
    let n = p.len();
    if n < 4 {
        return false;
    }
    for i in 0..n {
        for j in (i + 1)..n {
            // Benachbarte Seiten teilen einen Endpunkt
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            if kreuzen(p[i], p[(i + 1) % n], p[j], p[(j + 1) % n]) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quadrat() -> Vec<Punkt> {
        vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]
    }

    fn l_form() -> Vec<Punkt> {
        vec![(0.0, 0.0), (10.0, 0.0), (10.0, 4.0), (4.0, 4.0), (4.0, 10.0), (0.0, 10.0)]
    }

    #[test]
    fn flaeche_und_umfang() {
        assert!((flaeche(&quadrat()) - 100.0).abs() < 1e-9);
        assert!((umfang(&quadrat()) - 40.0).abs() < 1e-9);
        assert!((flaeche(&l_form()) - 64.0).abs() < 1e-9);
        let mut rueck = quadrat();
        rueck.reverse();
        assert!(flaeche_vorzeichen(&rueck) < 0.0 && (flaeche(&rueck) - 100.0).abs() < 1e-9);
        assert_eq!(flaeche(&quadrat()[..2]), 0.0);
    }

    #[test]
    fn grosse_koordinaten_bleiben_genau() {
        let q: Vec<Punkt> = quadrat().iter().map(|p| (p.0 + 4_400_000.0, p.1 + 5_300_000.0)).collect();
        assert!((flaeche(&q) - 100.0).abs() < 1e-6);
        let s = schwerpunkt(&q);
        assert!((s.0 - 4_400_005.0).abs() < 1e-6 && (s.1 - 5_300_005.0).abs() < 1e-6);
    }

    #[test]
    fn dreiecke_decken_auch_konkave_flaechen_ab() {
        for poly in [quadrat(), l_form(), l_form().into_iter().rev().collect()] {
            let tri = dreiecke(&poly);
            assert_eq!(tri.len(), poly.len() - 2);
            let summe: f64 = tri.iter().map(|t| flaeche(&[poly[t[0]], poly[t[1]], poly[t[2]]])).sum();
            assert!((summe - flaeche(&poly)).abs() < 1e-9, "{summe}");
        }
    }

    #[test]
    fn selbstschnitt_wird_erkannt() {
        assert!(!selbstschnitt(&quadrat()));
        assert!(!selbstschnitt(&l_form()));
        let schleife = vec![(0.0, 0.0), (10.0, 10.0), (10.0, 0.0), (0.0, 10.0)];
        assert!(selbstschnitt(&schleife));
        // Terminiert auch bei Selbstschnitt
        assert_eq!(dreiecke(&schleife).len(), 2);
    }

    #[test]
    fn schwerpunkt_der_l_form() {
        let s = schwerpunkt(&l_form());
        // Zerlegung: 10x4 (Schwerpunkt 5/2) und 4x6 (Schwerpunkt 2/7)
        let r = (40.0 * 5.0 + 24.0 * 2.0) / 64.0;
        let h = (40.0 * 2.0 + 24.0 * 7.0) / 64.0;
        assert!((s.0 - r).abs() < 1e-9 && (s.1 - h).abs() < 1e-9);
    }
}
