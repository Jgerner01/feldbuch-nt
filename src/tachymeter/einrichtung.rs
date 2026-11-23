// Zweck:    Gerät vor der ersten Messung einer Verbindung einstellen: Zieltyp/EDM-Programm (Prisma bzw. reflektorlos),
//           Prismenkonstante und Laserpointer nach den gespeicherten Einstellungen. Nicht beim Programmstart, weil die
//           Kommunikation dann ggf. noch nicht steht; nach Neustart oder Neuverbindung erneut vor der ersten Messung.
// Hardware: Leica TS06 / TPS300 / TPS1200 (GeoCOM, GSI Online)
// Autor:    Johann Gerner

use super::befehle::{Befehlsgeber, Protokoll};
use super::geocom;
use super::konfiguration::MessModus;
use crate::daten::einstellungen::Einstellungen;
use crate::grundlagen::format::f;

/// Millimeter pro Meter.
const MM_PRO_M: f64 = 1000.0;

/// Gewünschter Gerätezustand (aus den Einstellungen).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vorgabe {
    pub modus: MessModus,
    pub prismenkonstante_mm: f64,
    pub laser_an: bool,
}

impl Vorgabe {
    pub fn aus_einstellungen(e: &Einstellungen) -> Self {
        Self { modus: MessModus::aus_name(&e.mess_modus_name).unwrap_or(MessModus::Prisma), prismenkonstante_mm: e.prisma_konstante_mm, laser_an: e.laser_an }
    }

    /// Kurzbeschreibung für Protokolle.
    pub fn text(&self) -> String {
        let ziel = match self.modus {
            MessModus::Prisma => format!("Prisma ({} mm)", f(self.prismenkonstante_mm, 1)),
            MessModus::ReflektorlosKurz => "Reflektorlos kurz".to_string(),
            MessModus::ReflektorlosLang => "Reflektorlos lang".to_string(),
        };
        format!("{ziel}, Laserpointer {}", if self.laser_an { "an" } else { "aus" })
    }
}

/// Was ein erfolgreicher Schritt am bekannten Gerätezustand ändert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wirkung {
    Reflektorlos(bool),
    Laser(bool),
    Keine,
}

/// Ein Schritt der Einrichtung (ein Schaltvorgang mit Quittung je Befehl).
#[derive(Clone, Debug, PartialEq)]
pub struct Schritt {
    pub beschreibung: String,
    pub befehle: Vec<(String, i32)>,
    /// Scheitert ein kritischer Schritt, wird die Messung nicht ausgeführt.
    pub kritisch: bool,
    pub wirkung: Wirkung,
}

fn mit_rpc(befehle: Vec<String>) -> Vec<(String, i32)> {
    befehle.into_iter().map(|b| {
        let rpc = geocom::rpc_aus_anfrage(&b);
        (b, rpc)
    }).collect()
}

/// Schritte der Einrichtung und Hinweise für Einstellungen, die das Protokoll nicht setzen kann.
pub fn schritte(protokoll: Protokoll, v: &Vorgabe) -> (Vec<Schritt>, Vec<String>) {
    let bg = Befehlsgeber::fuer(protokoll);
    let mut schritte = Vec::new();
    let mut hinweise = Vec::new();
    let reflektorlos = v.modus != MessModus::Prisma;
    if let Some(befehle) = bg.edm_modus_befehle(i32::from(reflektorlos), v.modus.edm_programm()) {
        schritte.push(Schritt {
            beschreibung: format!("Ziel {}", if reflektorlos { "reflektorlos" } else { "Prisma" }),
            befehle: mit_rpc(befehle),
            kritisch: true,
            wirkung: Wirkung::Reflektorlos(reflektorlos),
        });
    }
    if v.modus == MessModus::Prisma {
        match protokoll {
            Protokoll::GeoCom => schritte.push(Schritt {
                beschreibung: format!("Prismenkonstante {} mm", f(v.prismenkonstante_mm, 1)),
                befehle: mit_rpc(vec![format!("%R1Q,{},0:{:.4}", geocom::RPC_TMC_SET_PRISM_CORR, v.prismenkonstante_mm / MM_PRO_M)]),
                kritisch: true,
                wirkung: Wirkung::Keine,
            }),
            Protokoll::GsiOnline => {
                hinweise.push(format!("Die Prismenkonstante ({} mm) kann über GSI Online nicht gesetzt werden – bitte am Gerät prüfen.", f(v.prismenkonstante_mm, 1)))
            }
            _ => {}
        }
    }
    if bg.unterstuetzt_laserpointer() {
        if let Some(b) = bg.laser_befehl(v.laser_an) {
            schritte.push(Schritt {
                beschreibung: format!("Laserpointer {}", if v.laser_an { "ein" } else { "aus" }),
                befehle: mit_rpc(vec![b]),
                // Nicht jedes Gerät hat einen Laserpointer: ein Fehler verhindert die Messung nicht
                kritisch: false,
                wirkung: Wirkung::Laser(v.laser_an),
            });
        }
    }
    (schritte, hinweise)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vorgabe(modus: MessModus, laser: bool) -> Vorgabe {
        Vorgabe { modus, prismenkonstante_mm: -17.5, laser_an: laser }
    }

    #[test]
    fn geocom_prisma_mit_konstante_und_laser() {
        let (s, h) = schritte(Protokoll::GeoCom, &vorgabe(MessModus::Prisma, true));
        assert!(h.is_empty());
        assert_eq!(s.len(), 3);
        assert!(s[0].kritisch && s[0].wirkung == Wirkung::Reflektorlos(false));
        assert_eq!(s[0].befehle.len(), 2);
        assert_eq!(s[1].befehle[0].0, "%R1Q,2024,0:-0.0175");
        assert_eq!(s[1].befehle[0].1, geocom::RPC_TMC_SET_PRISM_CORR);
        assert!(!s[2].kritisch && s[2].wirkung == Wirkung::Laser(true));
    }

    #[test]
    fn gsi_reflektorlos_ohne_prismenschritt() {
        let (s, h) = schritte(Protokoll::GsiOnline, &vorgabe(MessModus::ReflektorlosKurz, false));
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].befehle[0].0, "SET/161/7");
        assert_eq!(s[1].befehle[0].0, "SET/36/0");
        assert!(h.is_empty());
        // Prisma unter GSI: Hinweis statt Befehl
        let (s, h) = schritte(Protokoll::GsiOnline, &vorgabe(MessModus::Prisma, false));
        assert_eq!(s.len(), 2);
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn ohne_protokoll_keine_schritte() {
        let (s, h) = schritte(Protokoll::Keines, &vorgabe(MessModus::Prisma, true));
        assert!(s.is_empty() && h.is_empty());
    }
}
