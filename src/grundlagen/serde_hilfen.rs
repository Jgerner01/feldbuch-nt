// Zweck:    Serde-Hilfen für das JSON-Format der C#-Version (NaN als Text, DateTime mit 7 Nachkommastellen)
// Hardware: keine
// Autor:    Johann Gerner

/// `f64` mit NaN/Unendlich als Text ("NaN", "Infinity", "-Infinity"), wie System.Text.Json.
pub mod f64_text {
    use serde::de::{self, Visitor};
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(wert: &f64, s: S) -> Result<S::Ok, S::Error> {
        if wert.is_finite() {
            s.serialize_f64(*wert)
        } else if wert.is_nan() {
            s.serialize_str("NaN")
        } else if *wert > 0.0 {
            s.serialize_str("Infinity")
        } else {
            s.serialize_str("-Infinity")
        }
    }

    struct ZahlOderText;

    impl Visitor<'_> for ZahlOderText {
        type Value = f64;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("eine Zahl oder NaN/Infinity als Text")
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<f64, E> {
            Ok(v)
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<f64, E> {
            match v {
                "NaN" => Ok(f64::NAN),
                "Infinity" => Ok(f64::INFINITY),
                "-Infinity" => Ok(f64::NEG_INFINITY),
                _ => v.parse().map_err(de::Error::custom),
            }
        }
        fn visit_unit<E: de::Error>(self) -> Result<f64, E> {
            Ok(f64::NAN)
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        d.deserialize_any(ZahlOderText)
    }
}

/// `NaiveDateTime` im C#-Format ("2026-01-31T12:30:45.1234567", optional mit Zeitzone).
pub mod datum_zeit {
    use chrono::{DateTime, NaiveDateTime};
    use serde::{Deserialize, Deserializer, Serializer};

    const AUSGABE_FORMAT: &str = "%Y-%m-%dT%H:%M:%S";
    const EINGABE_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.f";
    /// Nanosekunden pro Tick (C# schreibt Zehntel-Mikrosekunden, also 7 Nachkommastellen).
    const NANOS_PRO_TICK: u32 = 100;

    pub fn serialize<S: Serializer>(wert: &NaiveDateTime, s: S) -> Result<S::Ok, S::Error> {
        let ticks = wert.and_utc().timestamp_subsec_nanos() / NANOS_PRO_TICK;
        s.serialize_str(&format!("{}.{ticks:07}", wert.format(AUSGABE_FORMAT)))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<NaiveDateTime, D::Error> {
        let text = String::deserialize(d)?;
        if let Ok(mit_zone) = DateTime::parse_from_rfc3339(&text) {
            return Ok(mit_zone.naive_local());
        }
        NaiveDateTime::parse_from_str(&text, EINGABE_FORMAT).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDateTime;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug)]
    struct Probe {
        #[serde(with = "super::f64_text")]
        wert: f64,
        #[serde(with = "super::datum_zeit")]
        zeit: NaiveDateTime,
    }

    #[test]
    fn nan_und_datum_runden_um() {
        let p: Probe =
            serde_json::from_str(r#"{"wert":"NaN","zeit":"2026-05-04T10:11:12.1234567"}"#).unwrap();
        assert!(p.wert.is_nan());
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"NaN\""));
        assert!(json.contains("2026-05-04T10:11:12.1234567"));
        let mit_zone: Probe =
            serde_json::from_str(r#"{"wert":1.5,"zeit":"2026-05-04T10:11:12+02:00"}"#).unwrap();
        assert_eq!(mit_zone.wert, 1.5);
    }
}
