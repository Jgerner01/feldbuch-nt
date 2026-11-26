//! Zweck:    Punktaufnahme in der Oberfläche: Qualitätsprüfung, Epochensammlung, Ergebnis.
//! Hardware: simpleRTK2B (u-blox ZED-F9P)
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/measure.rs), eigenständig gepflegt in Feldbuch-NT.

use crate::gnss::kern::config::*;
use crate::gnss::kern::geo::{Geographic, geo_to_utm};
use crate::gnss::kern::height::HeightModel;
use crate::gnss::kern::model::{Fix, Solution};
use crate::gnss::kern::survey::{self, Epoch, PointResult};
use std::collections::BTreeMap;

const MS_PER_WEEK: u32 = 604_800_000;

#[derive(Debug, Clone)]
pub struct Criteria {
    pub epochs: usize,
    pub min_seconds: f64,
    pub max_sigma_2d: f64,
    pub max_sigma_h: f64,
    pub max_age: f64,
    pub max_pdop: f64,
    pub min_sats: u8,
    /// bei Fix-Verlust gesammelte Epochen verwerfen
    pub restart_on_fix_loss: bool,
}

impl Default for Criteria {
    fn default() -> Self {
        Criteria {
            epochs: DEFAULT_EPOCHS,
            min_seconds: 0.0,
            max_sigma_2d: DEFAULT_MAX_SIGMA_2D_M,
            max_sigma_h: DEFAULT_MAX_SIGMA_H_M,
            max_age: DEFAULT_MAX_AGE_S,
            max_pdop: DEFAULT_MAX_PDOP,
            min_sats: DEFAULT_MIN_SATS,
            restart_on_fix_loss: false,
        }
    }
}

impl Criteria {
    /// Grund der Ablehnung; `None` bedeutet angenommen. Fehlende Angaben führen zur Ablehnung.
    pub fn check(&self, s: &Solution) -> Option<&'static str> {
        if s.fix != Fix::Fixed {
            return Some("kein RTK Fix");
        }
        if !s.complete {
            return Some("Epoche unvollständig");
        }
        let (Some(s2d), Some(sh)) = (s.sigma_2d(), s.sigma_h()) else {
            return Some("keine Genauigkeitsangabe");
        };
        if s2d > self.max_sigma_2d {
            return Some("σ 2D zu groß");
        }
        if sh > self.max_sigma_h {
            return Some("σ Höhe zu groß");
        }
        match s.age {
            None => return Some("Korrekturalter unbekannt"),
            Some(a) if a > self.max_age => return Some("Korrekturen zu alt"),
            Some(_) => {}
        }
        if s.pdop > self.max_pdop {
            return Some("PDOP zu groß");
        }
        if s.num_sv < self.min_sats {
            return Some("zu wenige Satelliten");
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct PointRequest {
    pub nr: String,
    pub code: String,
    pub remark: String,
    pub pole_height: f64,
    pub antenna_offset: f64,
}

#[derive(Debug, Clone)]
pub struct Measurement {
    pub request: PointRequest,
    pub criteria: Criteria,
    pub accepted: Vec<(Epoch, Solution)>,
    pub rejected: BTreeMap<&'static str, usize>,
    pub last_rejection: Option<&'static str>,
    pub restarts: u32,
    pub done: bool,
}

impl Measurement {
    pub fn new(request: PointRequest, criteria: Criteria) -> Self {
        Measurement {
            request,
            criteria,
            accepted: Vec::new(),
            rejected: BTreeMap::new(),
            last_rejection: None,
            restarts: 0,
            done: false,
        }
    }

    /// Zeitspanne der angenommenen Epochen [s].
    pub fn seconds(&self) -> f64 {
        match (self.accepted.first(), self.accepted.last()) {
            (Some(a), Some(b)) => {
                ((b.1.itow + MS_PER_WEEK - a.1.itow) % MS_PER_WEEK) as f64 / 1000.0
            }
            _ => 0.0,
        }
    }

    pub fn rejected_total(&self) -> usize {
        self.rejected.values().sum()
    }

    pub fn add(&mut self, s: &Solution) {
        if self.done {
            return;
        }
        if let Some(reason) = self.criteria.check(s) {
            *self.rejected.entry(reason).or_default() += 1;
            self.last_rejection = Some(reason);
            if self.criteria.restart_on_fix_loss && s.fix != Fix::Fixed && !self.accepted.is_empty()
            {
                self.accepted.clear();
                self.restarts += 1;
            }
            return;
        }
        let u = geo_to_utm(
            &Geographic {
                lat: s.lat,
                lon: s.lon,
                h: 0.0,
            },
            UTM_ZONE,
        );
        let antenna = self.request.pole_height + self.request.antenna_offset;
        let epoch = Epoch {
            east: u.east_prefixed(),
            north: u.north,
            h: s.h - antenna,
            lat: s.lat,
            lon: s.lon,
        };
        self.accepted.push((epoch, s.clone()));
        self.last_rejection = None;
        self.done = self.accepted.len() >= self.criteria.epochs
            && self.seconds() >= self.criteria.min_seconds;
    }

    /// Mittel und Streuung der angenommenen Epochen.
    pub fn result(&self, hm: &HeightModel) -> Option<PointResult> {
        if self.accepted.is_empty() {
            return None;
        }
        let epochs: Vec<Epoch> = self.accepted.iter().map(|a| a.0.clone()).collect();
        Some(survey::evaluate(
            &self.request.nr,
            &epochs,
            self.rejected_total(),
            hm,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gnss::kern::ubxmsg::Cov;

    fn good(itow: u32) -> Solution {
        Solution {
            itow,
            fix: Fix::Fixed,
            lat: 48.8,
            lon: 12.9,
            h: 400.0,
            num_sv: 20,
            pdop: 1.2,
            age: Some(1.0),
            complete: true,
            cov: Some(Cov {
                itow,
                valid: true,
                nn: 4e-5,
                ee: 4e-5,
                dd: 1e-4,
                ne: 0.0,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn rejects_missing_information() {
        let c = Criteria::default();
        assert_eq!(c.check(&good(0)), None);
        assert!(
            c.check(&Solution {
                cov: None,
                ..good(0)
            })
            .is_some()
        );
        assert!(
            c.check(&Solution {
                age: None,
                ..good(0)
            })
            .is_some()
        );
        assert!(
            c.check(&Solution {
                complete: false,
                ..good(0)
            })
            .is_some()
        );
        assert!(
            c.check(&Solution {
                fix: Fix::Float,
                ..good(0)
            })
            .is_some()
        );
    }

    #[test]
    fn collects_until_count_and_duration() {
        let request = PointRequest {
            nr: "1".into(),
            code: String::new(),
            remark: String::new(),
            pole_height: 2.0,
            antenna_offset: 0.1,
        };
        let c = Criteria {
            epochs: 2,
            min_seconds: 3.0,
            restart_on_fix_loss: true,
            ..Default::default()
        };
        let mut m = Measurement::new(request, c);
        m.add(&good(1000));
        m.add(&Solution {
            fix: Fix::Float,
            ..good(2000)
        });
        assert_eq!((m.accepted.len(), m.restarts), (0, 1));
        for t in [3000, 4000, 5000] {
            m.add(&good(t));
            assert!(!m.done);
        }
        m.add(&good(6000));
        assert!(m.done);
        let r = m.result(&HeightModel::None).unwrap();
        assert!((r.h_ell - 397.9).abs() < 1e-9);
    }
}
