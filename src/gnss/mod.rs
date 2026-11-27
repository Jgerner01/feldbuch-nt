// Zweck:    GNSS-Modul von Feldbuch-NT: RTK-Messung mit simpleRTK2B (u-blox ZED-F9P) und SAPOS HEPS.
//           Kern und Oberfläche sind aus gnss-vermessung 0.4.0 übernommen und werden hier eigenständig gepflegt
//           (Herkunft siehe HERKUNFT.md).
// Hardware: simpleRTK2B (u-blox ZED-F9P) an USB
// Autor:    Johann Gerner

pub mod ansicht;
pub mod bewegung;
pub mod einpassung;
pub mod kern;
pub mod punkte;
pub mod vorhersage;

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use super::einpassung::{EinpassMethode, Einpassung, HoehenEinpassung, TPunkt};
    use super::kern::config::UTM_ZONE;
    use super::kern::engine::{Engine, Source};
    use super::kern::height::HeightModel;
    use super::kern::measure::{Criteria, Measurement, PointRequest};

    /// Höchste Wartezeit auf die Wiedergabe.
    const WARTEZEIT: Duration = Duration::from_secs(60);
    /// Wiedergabegeschwindigkeit.
    const TEMPO: f64 = 50.0;

    /// Wiedergabe der Demo-Aufzeichnung: Messung mit Mittelung, Ergebnis in UTM32, Einpassung auf ein Plansystem.
    #[test]
    fn wiedergabe_messung_und_einpassung() {
        let pfad = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdaten/gnss/demo.ubx");
        let engine = Engine::start(Source::Replay { path: pfad, speed: TEMPO }, Arc::new(|| {})).expect("Wiedergabe");
        let request = PointRequest { nr: "1".into(), code: String::new(), remark: String::new(), pole_height: 2.0, antenna_offset: 0.0 };
        let kriterien = Criteria { epochs: 5, ..Criteria::default() };
        engine.state().measurement = Some(Measurement::new(request, kriterien));
        let start = Instant::now();
        let messung = loop {
            assert!(start.elapsed() < WARTEZEIT, "Messung nicht fertig geworden");
            {
                let mut st = engine.state();
                if st.measurement.as_ref().is_some_and(|m| m.done) {
                    break st.measurement.take().unwrap();
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let r = messung.result(&HeightModel::None).expect("Ergebnis");
        assert_eq!(r.n, 5);
        let e = r.east - f64::from(UTM_ZONE) * 1e6;
        assert!((200_000.0..900_000.0).contains(&e) && (5_200_000.0..5_600_000.0).contains(&r.north), "{e} {}", r.north);
        // Einpassung: Plan = UTM verschoben um (−600 000, −5 400 000) und 0,5 m Höhe
        let mut ep = Einpassung::neu("E".into(), EinpassMethode::Helmert2D, HoehenEinpassung::Verschiebung, "");
        for (i, (de, dn)) in [(0.0, 0.0), (50.0, 0.0), (0.0, 50.0)].iter().enumerate() {
            ep.punkte.push(TPunkt {
                nr: i.to_string(),
                soll_r: e + de - 600_000.0,
                soll_h: r.north + dn - 5_400_000.0,
                soll_hoehe: Some(r.h_ell + 0.5),
                ist_e: e + de,
                ist_n: r.north + dn,
                ist_hoehe: Some(r.h_ell),
                s_lage_mm: 0.0,
                s_hoehe_mm: 0.0,
                epochen: 5,
                belegungen: 1,
                lage_aktiv: true,
                hoehe_aktiv: true,
                zeit: String::new(),
            });
        }
        ep.berechne(None);
        let erg = ep.ergebnis.expect("Einpassung");
        let (rr, hh, hoehe) = erg.transformiere(e + 10.0, r.north + 10.0, Some(r.h_ell));
        assert!((rr - (e + 10.0 - 600_000.0)).abs() < 1e-4 && (hh - (r.north + 10.0 - 5_400_000.0)).abs() < 1e-4);
        assert!((hoehe.unwrap() - r.h_ell - 0.5).abs() < 1e-6);
    }
}
