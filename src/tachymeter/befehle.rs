// Zweck:    Protokollspezifische Befehle und Parser je Gerät; bei Leica-Geräten ist zwischen GeoCOM und
//           GSI Online umschaltbar
// Hardware: Leica TS06 / TPS300 / TPS1200 (GeoCOM, GSI Online), Topcon GTS
// Autor:    Johann Gerner

use super::messung::TachymeterMessung;
use super::modell::{Sprache, TachymeterModell};
use super::{geocom, gsi, topcon};

/// Protokollfamilie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protokoll {
    GeoCom,
    GsiOnline,
    Topcon,
    /// GNSS und manuelle Eingabe: keine Befehle, keine Parser.
    Keines,
}

impl Protokoll {
    /// Protokoll eines Geräts mit der gewählten Sprache (die Sprache wirkt nur bei Leica-Geräten).
    pub fn fuer(modell: TachymeterModell, sprache: Sprache) -> Self {
        match modell {
            TachymeterModell::TopconGts => Self::Topcon,
            TachymeterModell::GnssSapos | TachymeterModell::Manuell => Self::Keines,
            TachymeterModell::TrimbleS3 => Self::GeoCom,
            _ => match sprache {
                Sprache::GeoCom => Self::GeoCom,
                Sprache::GsiOnline => Self::GsiOnline,
            },
        }
    }

    /// Spricht das Gerät GeoCOM?
    pub fn ist_geocom(self) -> bool {
        self == Self::GeoCom
    }
}

/// Befehle des gewählten Geräts.
#[derive(Clone, Copy, Debug)]
pub struct Befehlsgeber {
    pub protokoll: Protokoll,
}

impl Befehlsgeber {
    pub fn fuer(protokoll: Protokoll) -> Self {
        Self { protokoll }
    }

    pub fn name(&self) -> &'static str {
        match self.protokoll {
            Protokoll::GeoCom => "Leica GeoCOM",
            Protokoll::GsiOnline => "Leica GSI Online",
            Protokoll::Topcon => "Topcon GTS/GPT",
            Protokoll::Keines => "Kein Protokoll",
        }
    }

    pub fn unterstuetzt_edm_modus(&self) -> bool {
        matches!(self.protokoll, Protokoll::GeoCom | Protokoll::GsiOnline)
    }

    pub fn unterstuetzt_winkel_live(&self) -> bool {
        matches!(self.protokoll, Protokoll::GeoCom | Protokoll::GsiOnline)
    }

    pub fn unterstuetzt_libelle_live(&self) -> bool {
        self.protokoll == Protokoll::GeoCom
    }

    pub fn unterstuetzt_laserpointer(&self) -> bool {
        matches!(self.protokoll, Protokoll::GeoCom | Protokoll::GsiOnline)
    }

    /// Schritt 1: Messung auslösen. Leerer Text = nur Zeilenende als Trigger; `None` = nicht möglich.
    pub fn mess_trigger_befehl(&self) -> Option<String> {
        match self.protokoll {
            Protokoll::GeoCom => Some(geocom::BEFEHL_MESS_TRIGGER.to_string()),
            Protokoll::GsiOnline => Some(gsi::BEFEHL_MESS_TRIGGER.to_string()),
            Protokoll::Topcon => Some(String::new()),
            Protokoll::Keines => None,
        }
    }

    /// Schritt 2 (nur GeoCOM): Messergebnis abrufen.
    pub fn mess_ergebnis_befehl(&self) -> Option<String> {
        (self.protokoll == Protokoll::GeoCom).then(|| geocom::BEFEHL_MESS_ERGEBNIS.to_string())
    }

    /// GeoCOM-RPC von Schritt 1 (0 bei anderen Protokollen).
    pub fn mess_schritt1_rpc(&self) -> i32 {
        if self.protokoll == Protokoll::GeoCom { geocom::RPC_TMC_DO_MEASURE } else { 0 }
    }

    /// GeoCOM-RPC von Schritt 2 (0 bei anderen Protokollen).
    pub fn mess_schritt2_rpc(&self) -> i32 {
        if self.protokoll == Protokoll::GeoCom { geocom::RPC_TMC_GET_SIMPLE_MEA } else { 0 }
    }

    pub fn winkel_befehl(&self) -> Option<String> {
        match self.protokoll {
            Protokoll::GeoCom => Some(geocom::BEFEHL_WINKEL.to_string()),
            Protokoll::GsiOnline => Some(gsi::BEFEHL_WINKEL.to_string()),
            _ => None,
        }
    }

    /// Nur Hz (für die Zielrichtungsanzeige); sonst wie `winkel_befehl`.
    pub fn hz_winkel_befehl(&self) -> Option<String> {
        match self.protokoll {
            Protokoll::GsiOnline => Some(gsi::BEFEHL_HZ_WINKEL.to_string()),
            _ => self.winkel_befehl(),
        }
    }

    pub fn winkel_rpc(&self) -> i32 {
        if self.protokoll == Protokoll::GeoCom { geocom::RPC_TMC_GET_ANGLE } else { 0 }
    }

    pub fn libelle_befehl(&self) -> Option<String> {
        (self.protokoll == Protokoll::GeoCom).then(|| geocom::BEFEHL_LIBELLE.to_string())
    }

    pub fn libelle_rpc(&self) -> i32 {
        if self.protokoll == Protokoll::GeoCom { geocom::RPC_TMC_GET_ANGLE1 } else { 0 }
    }

    pub fn laser_befehl(&self, an: bool) -> Option<String> {
        match self.protokoll {
            Protokoll::GeoCom => Some(geocom::laser_befehl(an)),
            Protokoll::GsiOnline => Some(gsi::laser_befehl(an)),
            _ => None,
        }
    }

    /// Befehle zur EDM-Umschaltung (Zieltyp 0 = Prisma, 1 = Reflektorlos).
    pub fn edm_modus_befehle(&self, ziel_typ: i32, edm_prog: i32) -> Option<Vec<String>> {
        match self.protokoll {
            Protokoll::GeoCom => Some(geocom::edm_modus_befehle(ziel_typ, edm_prog).to_vec()),
            Protokoll::GsiOnline => Some(vec![gsi::edm_modus_befehl(ziel_typ, edm_prog)]),
            _ => None,
        }
    }
}

/// Parser des gewählten Geräts; GeoCOM benötigt als Kontext den RPC der letzten Anfrage.
#[derive(Clone, Copy, Debug)]
pub struct DatenParser {
    pub protokoll: Protokoll,
    /// Kontext für GeoCOM-Antworten (RPC der letzten Anfrage, 0 = unbekannt).
    pub letzter_rpc: i32,
}

impl DatenParser {
    pub fn fuer(protokoll: Protokoll) -> Self {
        Self { protokoll, letzter_rpc: 0 }
    }

    pub fn setze_letzten_rpc(&mut self, rpc: i32) {
        self.letzter_rpc = rpc;
    }

    pub fn format_name(&self) -> &'static str {
        match self.protokoll {
            Protokoll::GeoCom => "GeoCOM",
            Protokoll::GsiOnline => "GSI-Online",
            Protokoll::Topcon => "Topcon",
            Protokoll::Keines => "",
        }
    }

    pub fn kann_verarbeiten(&self, zeile: &str) -> bool {
        match self.protokoll {
            Protokoll::GeoCom => geocom::kann_verarbeiten(zeile),
            Protokoll::GsiOnline => gsi::kann_verarbeiten_online(zeile),
            Protokoll::Topcon => topcon::kann_verarbeiten_topcon(zeile),
            Protokoll::Keines => false,
        }
    }

    pub fn parse_zeile(&self, zeile: &str) -> Option<TachymeterMessung> {
        match self.protokoll {
            Protokoll::GeoCom => geocom::parse_antwort(zeile.trim(), self.letzter_rpc),
            Protokoll::GsiOnline => gsi::parse_online_zeile(zeile),
            Protokoll::Topcon => topcon::parse_topcon_zeile(zeile),
            Protokoll::Keines => None,
        }
    }
}

/// Erkennt das Format einer Zeile automatisch.
pub fn parse_automatisch(zeile: &str) -> Option<TachymeterMessung> {
    if topcon::kann_verarbeiten_topcon(zeile) {
        return topcon::parse_topcon_zeile(zeile);
    }
    if gsi::kann_verarbeiten_online(zeile) {
        return gsi::parse_online_zeile(zeile);
    }
    if geocom::kann_verarbeiten(zeile) {
        return geocom::parse_antwort(zeile.trim(), 0);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protokoll_je_modell_und_sprache() {
        assert_eq!(Protokoll::fuer(TachymeterModell::LeicaTs06, Sprache::GeoCom), Protokoll::GeoCom);
        assert_eq!(Protokoll::fuer(TachymeterModell::LeicaTs06, Sprache::GsiOnline), Protokoll::GsiOnline);
        assert_eq!(Protokoll::fuer(TachymeterModell::LeicaTps300, Sprache::GsiOnline), Protokoll::GsiOnline);
        assert_eq!(Protokoll::fuer(TachymeterModell::LeicaTps300, Sprache::GeoCom), Protokoll::GeoCom);
        // Trimble S3 kennt nur GeoCOM, Topcon/GNSS/Manuell ignorieren die Sprache
        assert_eq!(Protokoll::fuer(TachymeterModell::TrimbleS3, Sprache::GsiOnline), Protokoll::GeoCom);
        assert_eq!(Protokoll::fuer(TachymeterModell::TopconGts, Sprache::GeoCom), Protokoll::Topcon);
        assert_eq!(Protokoll::fuer(TachymeterModell::Manuell, Sprache::GeoCom), Protokoll::Keines);
        assert!(Protokoll::GeoCom.ist_geocom() && !Protokoll::GsiOnline.ist_geocom());
    }

    #[test]
    fn befehle_wie_im_original() {
        let g = Befehlsgeber::fuer(Protokoll::GeoCom);
        assert_eq!(g.mess_trigger_befehl().as_deref(), Some("%R1Q,2008,0:1,1"));
        assert_eq!(g.mess_ergebnis_befehl().as_deref(), Some("%R1Q,2108,0:5000,1"));
        assert_eq!((g.mess_schritt1_rpc(), g.mess_schritt2_rpc()), (2008, 2108));
        assert_eq!(g.edm_modus_befehle(1, 5).unwrap(), vec!["%R1Q,17021,0:1", "%R1Q,2020,0:5"]);

        let o = Befehlsgeber::fuer(Protokoll::GsiOnline);
        assert_eq!(o.mess_trigger_befehl().as_deref(), Some("GET/M/WI21/WI22/WI31"));
        assert!(o.mess_ergebnis_befehl().is_none());
        assert_eq!(o.hz_winkel_befehl().as_deref(), Some("GET/I/WI21"));
        assert!(!o.unterstuetzt_libelle_live());

        let t = Befehlsgeber::fuer(Protokoll::Topcon);
        assert_eq!(t.mess_trigger_befehl().as_deref(), Some(""));
        assert!(t.laser_befehl(true).is_none());
        assert!(Befehlsgeber::fuer(Protokoll::Keines).mess_trigger_befehl().is_none());
    }

    #[test]
    fn parser_je_protokoll_und_automatik() {
        let mut p = DatenParser::fuer(Protokoll::GeoCom);
        p.setze_letzten_rpc(geocom::RPC_TMC_GET_ANGLE);
        let m = p.parse_zeile("%R1P,0,0:0,3.14159265358979,1.5707963267949").unwrap();
        assert!((m.hz_gon.unwrap() - 200.0).abs() < 1e-6);
        assert_eq!(DatenParser::fuer(Protokoll::GsiOnline).format_name(), "GSI-Online");
        assert!(parse_automatisch("%R1P,0,0:0,1.0,2.0,3.0").is_some());
        assert!(parse_automatisch("SS-H:90.0 V:90.0 D: 1.0").is_some());
        assert!(parse_automatisch("@E139").is_some());
        assert!(parse_automatisch("nichts").is_none());
    }
}
