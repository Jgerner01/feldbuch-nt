# Prüfvermerk zur Arbeitsfassung 1.0

Datum: 06.11.2026. Softwaregrundlage: gesicherter Quellstand Feldbuch-NT 0.5.2, Build 34.

## Durchgeführt

- Quellcodeprüfung des Viewers, der Stationierungsmaske, des Rechenkerns und der relevanten Import-/Speicher-/Exportpfade.
- Isolierte Rust-Testausführung mit dem unveränderten Berechnungscode, CSV-Parser und Zahlenformatierungsmodul des gesicherten Quellstands. Die benötigten Modelltypen wurden für diesen isolierten Aufbau ohne Serde-Attribute übernommen. Nicht das gesamte Anwendungsprojekt wurde gebaut.
- 21 Tests erfolgreich: 12 vorhandene Rechenkernprüfungen, 4 vorhandene Stationierungs-CSV-Prüfungen, 3 vorhandene Zahlenformatierungsprüfungen und 2 zusätzliche Prüfungen der mitgelieferten Übungsdateien.
- Vier eigens erstellte, schematische Grafiken; visuelle Prüfung der Abbildungen.
- Strukturprüfung von Markdown-Verweisen, Bilddateien und Archivinhalt.

## Ergebnisse der Übungsdateien

| Datensatz / Auswahl | Ergebnis |
| --- | --- |
| Fehlerfreie Demo, m = 1 | R = 1000,000000000 m; H = 2000,000000000 m; Höhe = 100,000000000 m; Orientierung = 25,000000000 gon; r = 5; s0 numerisch praktisch 0 |
| P3-Strecke +50 mm, alle Beobachtungen aktiv, m = 1 | R ungefähr 1000,000000 m; H ungefähr 2000,004083 m; s0 ungefähr 6,696345 mm; Längsresiduum P3 ungefähr +45,917455 mm |
| Gestörter Datensatz, P3-Strecke ausgeschaltet, m = 1 | R = 1000,000000000 m; H = 2000,000000000 m; r = 4; s0 numerisch praktisch 0 |

Die Fehlervariante verändert bei unverändertem V = 100 gon die Höhenbestimmung nicht. Die Übernahmefreigabe der GUI wurde im isolierten Rechenkernlauf nicht ausgeführt.

## Nicht durchgeführt

- Kein vollständiger Bedienablauf in der Windows-Oberfläche.
- Keine Geräteverbindung und keine realen Tachymetermessungen.
- Kein praktischer Persistenz-/Neustarttest der gesamten Anwendung.
- Kein vollständiger CAD- und PDF-Vergleich mit externen Programmen.
- Keine statistische Gesamtvalidierung des Ausgleichungsmodells.

Die Quellstände und Hashes in `quellstand.json` dienen als Vergleichsbasis für spätere Fortschreibungen. Sie identifizieren die geprüften Quelldateien, nicht eine bestimmte bereits vorhandene EXE.
