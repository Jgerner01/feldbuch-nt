# CSV-Import und -Export in Feldbuch-NT

Stand: 16.11.2026. Die Abschnitte 1–10 dokumentieren die ursprüngliche Bestandsanalyse und Formatspezifikation. Der Umsetzungsstand steht in Abschnitt 11.

## 1. Ziel und Ergebnis der Analyse

Die gewünschte Dateibenennung passt bereits zu einem Teil des Projekts:

| Dateifamilie | Vorgeschlagener Name | Inhalt |
| --- | --- | --- |
| Koordinaten | `<Name>-kor.csv` | Bekannte, digitalisierte, transformierte oder berechnete Punktkoordinaten |
| Messdaten | `<Name>-dat.csv` | Einzelbeobachtungen, gegebenenfalls von mehreren Standpunkten |
| Standpunktdatensatz | `<Name>-stp.csv` | Genau eine Aufstellung mit Instrumentenhöhe, Anschlusspunktkoordinaten und Beobachtungen; optional Stationierungsergebnis |

`<Name>` ist ein frei gewählter Basisname, nicht zwingend der Projektname. Beispiele: `Baustelle-kor.csv`, `Baustelle-dat.csv`, `Baustelle-SP01-20261116T093000-stp.csv`. Die Anführungszeichen aus der Aufgabenbeschreibung sind keine Zeichen des Dateinamens.

**Empfehlung:** Das vorhandene Abschnittsformat mit `# METADATA`, `# DATENTYP` und `# DATEN` als gemeinsame Grundlage verwenden. Die bestehenden KOR- und DAT-Kernspalten beibehalten und das bisherige Standpunktformat darauf ausrichten. CSV-Einlesen, Schreiben, Spaltenzuordnung und Dateibenennung müssen später zentral zusammengeführt werden.

Das bestehende Abschnittsformat ist ein projektspezifisches CSV-Dokument mit vorgeschaltetem Textkopf, keine durchgehend rechteckige CSV-Tabelle. Tabellenprogramme zeigen die Metadaten als zusätzliche Zeilen an. Der Vorschlag behält dies bewusst zugunsten der vorhandenen Formate bei; ein rein tabellarischer Fremdexport wäre eine gesonderte Option.

## 2. Module und Funktionen im Istzustand

Die Pfade sind relativ zum Projektverzeichnis. „Import/Export“ umfasst auch automatisch gelesene oder geschriebene Projektdateien.

### 2.1 Datenmodule

| Modul | Funktionen / Einstiegspunkte | Richtung und derzeitiges Format |
| --- | --- | --- |
| `src/daten/csv_datei.rs` | `CsvDatei::neu`, `laden`, `speichern`, `normalisiere_pfad` | Standarddateien für Koordinaten und Messdaten mit Abschnittskopf und Semikolon; frei bearbeitbare Spalten |
| `src/daten/csv_datei.rs` | `importiere_kor`, `importiere_dat`, `exportiere_kor`, `exportiere_dat` | Brücke zwischen Standard-CSV und alten leerzeichengetrennten `.kor`-/`.dat`-Dateien |
| `src/daten/kor_csv.rs` | `erzeuge`, `lese_punkte`, `schreibe` | KOR-CSV erzeugen, lesen und zusammenführen; neue Punkte ersetzen gleiche Nummern ohne Beachtung der Groß-/Kleinschreibung |
| `src/daten/messdaten.rs` | `messdatei_pfad`, `schreibe_neupunktmessung`, `schreibe_stationierungsmessung`, `schreibe_standpunktinfo` | Projektweite `<Projekt>-dat.csv`; AP-/NP-Messungen und Stationierungs-Kommentarzeilen |
| `src/daten/messdaten.rs` | `migriere_spalten`, `schreibe_exzentrizitaeten` | Alte sieben Spalten auf elf erweitern; Exzentrizitäten positionsbezogen zurückschreiben |
| `src/daten/messdaten.rs` | `tagesprotokoll_pfad`, `schreibe_tagesprotokoll` | Automatischer Export nach `Data-protocol-YYYY-MM-DD.csv`, einschließlich Geräte- und Qualitätsdaten |
| `src/daten/polar_messdaten.rs` | `lese`, `parse` | DAT-CSV mit sieben oder elf Spalten für Polarpunktberechnung; Standpunkt- und Typrekonstruktion für Altdateien |
| `src/daten/stationierung_csv.rs` | `parse` | Import von Stationierungsdatensätzen; Komma oder Semikolon, Spaltennamen/Aliase, optionale Standpunktnummer und Instrumentenhöhe |
| `src/daten/punktlisten.rs` | `lese_csv`, `lese_kor`, `lese_kor_roh` | Koordinatenimport; CSV mit Komma oder Semikolon, Kopfzeilensuche und Namensaliasen. Beim KOR-Import wird eine Begleitdatei `*-kor.csv` bevorzugt, wenn sie mindestens ebenso viele Punkte enthält |
| `src/daten/aufnahmetest.rs` | `kor_zeilen`, `dat_zeilen`, `export_pfade` | Paarweiser KOR-/DAT-CSV-Export des Aufnahmetests; DAT noch im siebenstelligen Spaltenschema |
| `src/daten/konvertierung.rs` | `csv_text` | Gemischter Tabellenexport aus Koordinaten und Messwerten, ohne Metadatenkopf |
| `src/daten/konvertierung.rs` | `kor_text`, `dat_text` | Zusätzlich alte Textformate; insbesondere `dat_text` ist kein zum DAT-Importer des Dateneditors passendes Messdatenformat |
| `src/daten/projekt.rs` | `anschlusspunkte_pfad`, `finde_anschlusspunkt` | Verwaltung und Koordinaten-Nachschlagen aus `Anschlusspunkte.csv`; eigener positionsgebundener Kommaparser |
| `src/daten/projektdaten.rs` | `ProjektdatenManager::initialisiere`, `laden`, `speichern`, `set_value`, `set_values`, `replace_all`; intern `parse_csv` | `Projektdaten.csv` als Historie von Projektparametern; Kommaformat mit vollständigerer Maskierung |

`src/daten/modelle.rs` enthält die zugehörigen Datenstrukturen, aber keine eigenen CSV-Dateizugriffe. Wichtig sind `StationierungsPunkt`, `StationierungsErgebnis`, `NeupunktRohdaten`, `NeupunktErgebnis`, `ImportPunkt` und `KonvertierungPunkt`.

### 2.2 Oberflächen und aufrufende Module

| Modul | Funktionen / Bedienweg | CSV-Nutzung |
| --- | --- | --- |
| `src/gui/daten_editor.rs` | `oeffnen`, `neue_datei`, `speichern`, `importieren`, `exportieren` | Zwei Editoren für KOR-/DAT-CSV über `CsvDatei`; Import/Export alter KOR-/DAT-Texte. Noch kein STP-Datentyp |
| `src/gui/freie_stationierung.rs` | `waehle_csv`, `lade_csv`, `lade_testdaten` | Expliziter Stationierungsimport über `stationierung_csv::parse`, auch `Muster.csv` |
| `src/gui/freie_stationierung.rs` | `lade_anschlusspunkte`, `fuelle_zeilen`, `beobachte_anschlusspunkte` | Automatisches Nachladen von `Anschlusspunkte.csv` über separaten Kommaparser |
| `src/gui/freie_stationierung.rs` | `tabellen_csv`, `standpunkt_datei_zeilen`, `standpunkt_csv_pfad`, `schreibe_csv`, `zeile_entfernen`, `neuer_standpunkt` | Standpunktexport als `Standpunkt-<Nr>.csv`, Änderungen der Anschlussdatei, zeitgestempelte Archivierung |
| `src/gui/dxf_viewer/zeichnen.rs` | `import_kor_csv`, `digitalisierung_umschalten`, `schreibe_digitalisier_punkt`, Hilfsfunktion `kor_csv_pfad` | Koordinatenimport und Digitalisierung; schreibt `.kor` und begleitende `*-kor.csv` parallel |
| `src/gui/dxf_viewer/messdaten_eingabe.rs` | `schreibe_anschlusspunkt` | Schreibt Koordinaten und Messwerte nach `Anschlusspunkte.csv`; Mehrfachmessungen gleicher Nummer bleiben erhalten |
| `src/gui/dxf_viewer/messen.rs` | Messverarbeitung, Aufrufe von `messdaten::schreibe_*` | Automatische Ausgabe des Tagesprotokolls, der AP-/NP-Messungen und der Stationierungsinformationen; Weitergabe von Anschlussmessungen |
| `src/gui/polarpunkte.rs` | `waehle_csv`, Laden über `polar_messdaten::lese`, Rückschreiben über `schreibe_exzentrizitaeten`, `kor_csv` | Messdatenimport, Exzentrizitätsänderung, Koordinatenexport über `kor_csv::schreibe` |
| `src/gui/koord_transformation.rs` | `importiere`, `exportiere` | Koordinatenimport über `punktlisten`; Export als `.kor` plus `*-kor.csv`, eigener CSV-Textaufbau |
| `src/gui/flaechenteilung.rs` | `lade_csv` | Eigener Koordinatenimport: Punktnummer wird übergangen, Spalten 2 und 3 werden als R/H gelesen; Reihenfolge bestimmt Polygon |
| `src/gui/aufnahmetest.rs` | Exportdialog und Aufruf von `aufnahmetest::{kor_zeilen, dat_zeilen, export_pfade}` | Speichert Koordinaten und zugehörige Messdaten in zwei CSV-Dateien |
| `src/gui/konvertierung.rs` | Schaltfläche „Export CSV …“, `exportiere` | Exportiert `csv_text` standardmäßig als `Punkte.csv`; weitere Schaltflächen schreiben `.kor` und `.dat` |
| `src/gui/projektdaten_ansicht.rs` | Projektdaten bearbeiten | Indirekter Import/Export über `ProjektdatenManager` |

`src/grundlagen/datei_speicher.rs` stellt gemeinsame Schreib- und Fehlermeldefunktionen bereit, definiert aber kein CSV-Schema. `src/grundlagen/format.rs` liefert `f` für Dezimalpunkte und `parse_zahl` für Punkt/Komma sowie die Ablehnung nicht endlicher Zahlen.

### 2.3 Abgrenzung und Beispieldateien

- `release/Muster.csv`: achtspaltiges Kommaformat für die freie Stationierung.
- `anleitung-dxf-viewer-paket/anleitung-dxf-viewer/beispieldaten/`: Koordinaten- und Anschlusspunktbeispiele einschließlich einer Fehlerdatei; bei einer späteren Vereinheitlichung mitführen.
- `src/dxf/exporter.rs` und `src/gui/dxf_viewer/export.rs` exportieren DXF, keine CSV-Dateien. CSV-/KOR-Dateinamen kommen dort unter anderem als Quell- oder Layernamen vor.
- `src/update.rs` verwendet die Windows-Prozessausgabeoption `/FO CSV`; dies ist kein fachlicher CSV-Import oder -Export.
- JSON-/XML-Projektspeicher und RTF-/PDF-Protokolle sind keine CSV-Schnittstellen. Eine vollständige Ablösung dieser Speicher ist durch die gewünschte Vereinheitlichung der CSV-Dateien nicht automatisch abgedeckt.

## 3. Bestehende Datenformate

### 3.1 Koordinaten

Das am besten geeignete vorhandene Grundformat steht in `csv_datei.rs` und `kor_csv.rs`:

```text
# METADATA
Projekt: Baustelle
Sensor: LeicaTS06
Bearbeiter:
Datum: 2026-11-16
---
# DATENTYP
Koordinaten
# DATEN
PunktNr; X; Y; Z; Code
1001; 1100.000; 2050.000; 452.000; FP
```

Im Projekt bedeutet **X = Rechtswert, Y = Hochwert, Z = Höhe**, jeweils in Metern. Diese Zuordnung ausdrücklich beibehalten; andere Vermessungsprogramme verwenden teilweise andere X-/Y-Konventionen. Die meisten Ausgaben verwenden drei Koordinatennachkommastellen, der Aufnahmetest vier.

### 3.2 Messdaten

Der aktuelle Standard hat elf Spalten:

```text
PunktNr; HZ_gon; V_gon; Strecke_m; Zielhoehe_m; Code; Zeitstempel; ExzLaengs_m; ExzQuer_m; StandpunktNr; Typ
```

`HZ_gon` ist die unorientierte Horizontalrichtung, `V_gon` der Zenitwinkel, `Strecke_m` die Schrägstrecke. `Typ` enthält `AP` oder `NP`. Der Metadatenkopf enthält zusätzlich `StandpunktNr`, `InstrumentenHoehe` und `StandpunktCode`.

Altdateien und der Aufnahmetest verwenden nur die ersten sieben Spalten. Die Messdatenschreibfunktion kann siebenstellige Altdateien mit Sicherung auf elf Spalten erweitern. Standpunktergebnisse werden zusätzlich als `# STATIONIERUNG: SP=... IH=... R=... H=... Hoehe=... Ori=... s0=... n=...` angehängt.

### 3.3 Standpunkt- und Anschlussdaten

`Anschlusspunkte.csv` und `Muster.csv`:

```text
PunktNr,R,H,Hoehe,HZ,V,Strecke,Zielhoehe
1001,1100.000,2050.000,452.000,55.4833,98.8039,111.822,1.600
```

`Standpunkt-<Nr>.csv` enthält zusätzlich vor der Tabelle:

```text
# METADATA
StandpunktNr: SP01
Instrumentenhoehe: 1.600
# DATEN
```

Die Zeilen beschreiben **Anschlusspunkte**, nicht die Koordinate des Standpunkts selbst. Ein berechnetes Stationierungsergebnis wird in diesem CSV-Export derzeit nicht mitgespeichert. Wiederholte Beobachtungen derselben Punktnummer sind fachlich zulässig.

### 3.4 Weitere CSV-Formate

- Konvertierung: `PunktNr;Typ;R;H;Hoehe;HZ;V;Strecke;Zielhoehe;Punktcode;Bemerkung`. Die Spalte `Typ` enthält beispielsweise Herkunfts-/Tabellentypen wie `Import-GSI` und ist nicht automatisch gleichbedeutend mit `AP`/`NP`.
- Projektdaten: `Datum,Uhrzeit,Bearbeiter,Kategorie,Parameter,Wert`; Parameterhistorie, keine Punktliste.
- Tagesprotokoll: 28 Semikolonspalten für Zeit, Standpunkt, Instrumentenhöhe, Punkt, Code, Methode, EDM, Prisma, Atmosphäre, Neigungen, Kompensator, Messwerte, Quelle und GNSS-Platzhalter. Dafür existiert in den untersuchten CSV-Modulen kein entsprechender fachlicher Rückimport.

## 4. Probleme, die eine Vereinheitlichung lösen sollte

1. **Mehrere Parser und Schreiber:** KOR-/DAT-Dateien werden an mehreren Stellen unabhängig aufgebaut. Komma-/Semikolonbehandlung, Metadaten und Nachkommastellen unterscheiden sich.
2. **Spaltenreihenfolge:** `polar_messdaten`, `kor_csv::lese_punkte`, Anschlussdatei-Nachladen und Flächenteilung lesen feste Positionen. Der Dateneditor erlaubt dagegen geänderte Spalten. Eine im Editor gespeicherte Umordnung kann deshalb in anderen Modulen falsch interpretiert werden.
3. **Maskierung:** Die meisten Leser verwenden `split`, die meisten Schreiber `join` oder Formattexte. Semikolon, Anführungszeichen und Zeilenumbrüche in Codes/Bemerkungen sind dort nicht zuverlässig darstellbar. Der Stationierungsparser unterstützt zitierte Felder innerhalb einer Zeile; der Projektdatenparser auch Zeilenumbrüche innerhalb zitierter Felder.
4. **Datenverlust:** `punktlisten::lese_csv` übernimmt keinen Code, da `ImportPunkt` keinen Code enthält. Fehlende Höhe wird dort zu `0.0`; `KorPunkt` kann fehlende Höhe dagegen als `None` erhalten. Ein bloßer Austausch des Parsers genügt daher nicht.
5. **Standpunktwechsel:** Eine projektweite DAT-Datei kann mehrere Standpunkte enthalten, hat aber nur eine Instrumentenhöhe im Kopf. Wiederholte Aufstellungen unter gleicher Punktnummer sind nicht eindeutig unterscheidbar. Kommentarzeilen sind kein belastbarer Ersatz für eine explizite Aufstellungskennung.
6. **Kommentare im Dateneditor:** `CsvDatei::laden` behandelt jede `#`-Zeile als Abschnittswechsel. Ein `# STATIONIERUNG:`-Kommentar beendet dadurch faktisch das Einlesen des laufenden Datenabschnitts bis zum nächsten passenden Abschnitt. Kommentar und nachfolgende Messungen können beim erneuten Speichern verloren gehen.
7. **Stille Auslassungen:** Mehrere Leser überspringen unlesbare oder zu kurze Datensätze ohne Fehlerbericht; der Polarleser ersetzt bestimmte fehlende Werte durch null und schätzt alte AP-/NP-Typen.
8. **Dateibenennung:** `normalisiere_pfad` akzeptiert vorhandene `-kor`- und `-dat`-Suffixe unabhängig vom gewünschten Datentyp; ein falsches Suffix wird nicht korrigiert. `aufnahmetest::export_pfade` normalisiert den gewählten KOR-Pfad selbst nicht. STP wird bisher nicht berücksichtigt.
9. **Trennzeichenerkennung:** `punktlisten::lese_csv` sucht Semikola im gesamten Dokument, auch in Metadaten. Die Flächenteilung trennt gleichzeitig an Komma und Semikolon und kann dadurch Dezimalkommas zerlegen.
10. **Gleichnamige DAT-Texte mit anderer Bedeutung:** `konvertierung::dat_text` schreibt `STANDPUNKT ...` und Koordinatenzeilen; `CsvDatei::importiere_dat` erwartet `-<Nr> <Instrumentenhöhe>` und danach Winkel/Strecke/Zielhöhe. Diese Formate sind nicht gegenseitig austauschbar. Der Importer überspringt außerdem die erste nichtleere Zeile auch ohne gültige Standpunktkennung und setzt Importzeitstempel statt ursprünglicher Messzeiten.

## 5. Vorschlag für gemeinsame Formatregeln

Die Regeln in diesem und den folgenden Abschnitten beschreiben das **Soll**, nicht die heutige Unterstützung.

| Eigenschaft | Vorschlag |
| --- | --- |
| Kodierung | UTF-8 ohne BOM beim Schreiben; UTF-8 mit und ohne BOM beim Lesen |
| Zeilenende | CRLF beim Schreiben; CRLF und LF beim Lesen |
| Trennzeichen | Semikolon; kanonischer Export ohne zusätzliche Trenn-Leerzeichen, Import toleriert bisheriges `; ` |
| Zahlen | Dezimalpunkt, keine Tausendertrennzeichen; Dezimalkomma nur beim kompatiblen Import |
| Einheiten | Koordinaten, Höhen, Strecken und Exzentrizitäten in m; Winkel in gon |
| Präzision | Koordinaten mindestens vier, Winkel mindestens vier, Strecken/Höhen/Exzentrizitäten mindestens drei Nachkommastellen; vorhandene höhere Präzision beim Durchreichen nicht reduzieren |
| Leere Felder | Unbekannt/nicht vorhanden; strikt von echtem Zahlenwert null unterscheiden |
| Textfelder | Bei Semikolon, Anführungszeichen oder Zeilenumbruch zitieren; innere `"` verdoppeln; mehrzeilige zitierte Felder unterstützen |
| Punktnummern | Text; führende Nullen erhalten, keine automatische Zahlenkonvertierung |
| Datum/Zeit | Datum `YYYY-MM-DD`; neue Messzeitstempel ISO 8601 mit Offset, etwa `2026-11-16T09:30:00+01:00`; alte Zeitwerte weiter lesen, nicht nachträglich als eindeutig bekannte Zeitzone ausgeben |
| Schema | Pflichtschlüssel `Formatversion: 1.0` im Metadatenkopf; ohne Version als Altformat behandeln |
| Spalten | Über Namen/Aliase zuordnen; Export in festgelegter Reihenfolge; doppelte kanonische Spaltennamen ablehnen |
| Erweiterungen | Unbekannte Spalten und Metadaten beim Bearbeiten erhalten; für Berechnungen nur bekannte Felder verwenden |
| Fehler | Zeile, Spalte und Grund melden; keine unbemerkte Nullsetzung oder Auslassung ungültiger Pflichtwerte |

Die Präzisionsregel vereinheitlicht künftige Ausgaben, stellt aber bereits gerundete Daten nicht wieder her.

Gemeinsame Metadaten: `Formatversion`, `Projekt`, `Sensor`, `Bearbeiter`, `Datum`. Optional `Koordinatensystem` und `Hoehensystem` als Beschreibung oder Kennung; unbekannte Systeme bleiben leer. Bei lokalen Aufnahmen `Koordinatensystem: Lokal` setzen. Keine Koordinatentransformation allein aus einem Dateinamen ableiten.

`---` bleibt als vorhandene Trennzeile erhalten. Nur bekannte Abschnittsmarker wechseln den Abschnitt. Sonstige Kommentarzeilen bleiben Kommentare und dürfen nachfolgende Daten nicht verschlucken.

## 6. Sollformat `<Name>-kor.csv`

Die fünf vorhandenen Kernspalten bleiben unverändert:

| Spalte | Bedeutung / Pflicht |
| --- | --- |
| `PunktNr` | Eindeutige Punktkennung innerhalb dieser Koordinatenliste; Pflicht |
| `X` | Rechtswert in m; Pflicht |
| `Y` | Hochwert in m; Pflicht |
| `Z` | Höhe in m; leer bei reinen Lagepunkten |
| `Code` | Fachlicher Punktcode; darf leer sein |

Optionale Erweiterungen nach den Kernspalten: `Bemerkung`, `Quelle`, `Zeitstempel`, `PunktTyp`. `PunktTyp` verhindert eine Verwechslung mit dem Messdatentyp `AP`/`NP`.

```text
# METADATA
Formatversion: 1.0
Projekt: Baustelle
Sensor: LeicaTS06
Bearbeiter: Hans
Datum: 2026-11-16
Koordinatensystem: Lokal
Hoehensystem:
---
# DATENTYP
Koordinaten
# DATEN
PunktNr;X;Y;Z;Code
001;1100.0000;2050.0000;452.0000;FP
002;1080.0000;1920.0000;;"Baum; Bestand"
```

Beim Import mehrerer Listen sind Kollisionen gleicher Punktnummer ausdrücklich zu behandeln: identisch überspringen, Konflikt melden oder bewusst ersetzen. Koordinatengleichheit allein soll nicht verschiedene fachliche Punktnummern entfernen. Die bisherige Zusammenführung in `kor_csv::schreibe` ist eine Exportoption, keine allgemeingültige Importregel.

## 7. Sollformat `<Name>-dat.csv`

Die bestehenden elf Spalten bleiben am Anfang erhalten:

```text
PunktNr;HZ_gon;V_gon;Strecke_m;Zielhoehe_m;Code;Zeitstempel;ExzLaengs_m;ExzQuer_m;StandpunktNr;Typ
```

Für eine eindeutige Zuordnung werden anschließend `AufstellungId` und `InstrumentenHoehe_m` empfohlen. Sie gehören zum neuen Standardexport. Damit ist die Instrumentenhöhe auch bei mehreren Aufstellungen nicht allein von einem Dateikopf abhängig.

| Feldgruppe | Regeln |
| --- | --- |
| Punkt und Beobachtung | `PunktNr`, `HZ_gon`, `V_gon`, `Strecke_m`, `Zielhoehe_m` für vollständige polare Beobachtungen erforderlich |
| Richtung | HZ unorientiert; V als Zenitwinkel. Zweite Fernrohrlage bleibt als gemessener Wert erhalten |
| Strecke | Schrägstrecke, grundsätzlich nicht negativ |
| Exzentrizitäten | Vorzeichen und bestehende Berechnungskonvention beibehalten; null nur bei tatsächlich fehlender Exzentrizität, leer bei unbekannt |
| `Typ` | Neue fachliche Exporte setzen ausdrücklich `AP` oder `NP`; unbekannte Alttypen beim Import melden/zuordnen, keine unbemerkte Schätzung |
| `StandpunktNr` | Fachliche Nummer des Instrumentenstandpunkts |
| `AufstellungId` | Innerhalb des Projekts eindeutige Kennung einer konkreten Aufstellung; gleiche Standpunktnummer kann mehrere Aufstellungen haben |
| `InstrumentenHoehe_m` | Instrumentenhöhe dieser Beobachtung; muss zum STP-Datensatz der Aufstellung passen |
| Wiederholungen | Mehrere Messungen derselben Punktnummer erhalten, weder nach Punktnummer noch nach Koordinate zusammenfassen |

Für Archivierung und Zusammenführung ist zusätzlich eine `MessungId` sinnvoll. Eine fehlende ID in Altdateien darf nicht zu vermeintlich sicherer Duplikaterkennung führen. Optional können `Bemerkung`, `Quelle` und `Messmethode` folgen. Reine Richtungsbeobachtungen mit leerer Strecke sind als solche zu erhalten; ein Modul, das eine vollständige Polarmessung benötigt, muss sie gezielt zurückweisen.

```text
# METADATA
Formatversion: 1.0
Projekt: Baustelle
Sensor: LeicaTS06
Bearbeiter: Hans
Datum: 2026-11-16
---
# DATENTYP
Messdaten
# DATEN
PunktNr;HZ_gon;V_gon;Strecke_m;Zielhoehe_m;Code;Zeitstempel;ExzLaengs_m;ExzQuer_m;StandpunktNr;Typ;AufstellungId;InstrumentenHoehe_m
1001;55.4833;98.8039;111.822;1.600;FP;2026-11-16T09:30:00+01:00;0.000;0.000;SP01;AP;SP01-20261116T093000;1.600
2001;200.0000;95.0000;20.000;1.500;Baum;2026-11-16T09:35:00+01:00;0.250;-0.100;SP01;NP;SP01-20261116T093000;1.600
```

Die bisherigen Kopfwerte `StandpunktNr`, `InstrumentenHoehe`, `StandpunktCode` dürfen für Ein-Aufstellungs-Dateien weiter erscheinen. Sie sind dann Vorgabewerte; Zeilenwerte dürfen ihnen nicht widersprechen. Bei mehreren Aufstellungen keine irreführende einzelne Kopf-Instrumentenhöhe ausgeben.

AP-Beobachtungen in DAT enthalten weiterhin keine bekannten Zielkoordinaten. Für eine neue Stationierungsberechnung werden zusätzlich KOR-Punkte oder ein vollständiger STP-Datensatz benötigt. Ein DAT-Export allein ist daher kein vollständiger Stationierungsexport.

## 8. Sollformat `<Name>-stp.csv`

Vorgeschlagene Bedeutung: **ein Datensatzpaket für genau eine Aufstellung**, zunächst die vorhandene Kombination aus Anschlusskoordinaten und Beobachtungen. Die Tabelle kann optional auch Neupunktbeobachtungen derselben Aufstellung enthalten; diese werden durch `Typ=NP` gekennzeichnet. Dadurch bleibt der Begriff „Datensätze von einem Standpunkt“ auch für einen vollständigen Beobachtungsexport nutzbar.

Die ersten acht Spalten entsprechen fachlich dem bestehenden Standpunktformat. Die Namen werden an KOR und DAT angeglichen:

| Bisher | Künftig | Bedeutung |
| --- | --- | --- |
| `PunktNr` | `PunktNr` | Beobachteter Zielpunkt |
| `R` | `X` | Bekannter Rechtswert des Anschlusspunkts |
| `H` | `Y` | Bekannter Hochwert des Anschlusspunkts |
| `Hoehe` | `Z` | Bekannte Höhe des Anschlusspunkts |
| `HZ` | `HZ_gon` | Gemessene Horizontalrichtung |
| `V` | `V_gon` | Gemessener Zenitwinkel |
| `Strecke` | `Strecke_m` | Gemessene Schrägstrecke |
| `Zielhoehe` | `Zielhoehe_m` | Zielhöhe |

Anschließend `Code`, `Zeitstempel`, `ExzLaengs_m`, `ExzQuer_m`, `Typ`; optional `MessungId` und `Bemerkung`. `StandpunktNr`, `AufstellungId` und `InstrumentenHoehe` stehen eindeutig im Kopf. Beim Export als DAT werden sie in die jeweiligen Zeilenspalten übernommen.

```text
# METADATA
Formatversion: 1.0
Projekt: Baustelle
Sensor: LeicaTS06
Bearbeiter: Hans
Datum: 2026-11-16
StandpunktNr: SP01
AufstellungId: SP01-20261116T093000
InstrumentenHoehe: 1.600
StandpunktCode: ST
Koordinatensystem: Lokal
Hoehensystem:
---
# DATENTYP
Standpunkt
# DATEN
PunktNr;X;Y;Z;HZ_gon;V_gon;Strecke_m;Zielhoehe_m;Code;Zeitstempel;ExzLaengs_m;ExzQuer_m;Typ
1001;1100.0000;2050.0000;452.0000;55.4833;98.8039;111.822;1.600;FP;2026-11-16T09:30:00+01:00;0.000;0.000;AP
1001;1100.0000;2050.0000;452.0000;255.4833;301.1961;111.823;1.600;FP;2026-11-16T09:31:00+01:00;0.000;0.000;AP
2001;;;;200.0000;95.0000;20.000;1.500;Baum;2026-11-16T09:35:00+01:00;0.250;-0.100;NP
```

Für AP sind X/Y erforderlich; Z ist für eine Höhenberechnung erforderlich, darf für reine Lageverwendung fehlen. NP-Koordinaten bleiben im STP-Beobachtungsteil leer, damit berechnete Ergebnisse nicht als bekannte Anschlusskoordinaten erscheinen. Berechnete Neupunktkoordinaten gehören nach KOR. Wiederholte AP-Zeilen bleiben erhalten; bei widersprüchlichen bekannten Koordinaten derselben Punktnummer ist ein Konflikt zu melden.

Der Import in die freie Stationierung übernimmt ausschließlich AP-Beobachtungen. NP-Zeilen bleiben im Dokument erhalten, werden jedoch nicht in die Anschlussausgleichung eingespeist. Das erfordert eine Erweiterung gegenüber dem heutigen achtspaltigen Parser.

### Optionales Stationierungsergebnis

Ein berechnetes Ergebnis kann als zusätzlicher, einzeiliger Abschnitt abgelegt werden. Ohne Ergebnisabschnitt ist die STP-Datei ein gültiger Eingabedatensatz. Beispiel mit rein illustrativen Ergebniswerten:

```text
# STATIONIERUNG
X;Y;Z;Orientierung_gon;Massstab;s0_mm;Redundanz
1000.0000;2000.0000;450.0000;12.3456;1.000000;1.20;3
```

Die Standpunkthöhe Z bezeichnet die Höhe des Bodenpunkts; die Instrumentenhöhe wird nicht eingerechnet. Die Bedeutung von Orientierung und Maßstab muss exakt der vorhandenen `StationierungsErgebnis`-Struktur folgen. Die Spalte `Redundanz` ist keine bloße Anzahl der Anschlusspunkte; die bisherige Kommentarabkürzung `n` sollte dafür nicht übernommen werden.

Dieser Abschnitt ist ein Ergebnis zur Nachvollziehbarkeit, kein Ersatz für eine vollständige Sicherung der Stationierungsberechnung. Aktivierungen, Gewichtungen, Rechenparameter, Residuen und Kovarianzen liegen teilweise in den JSON-Datenstrukturen. Soll STP später diese vollständige Sicherung leisten, müssen dafür weitere klar versionierte Felder/Abschnitte definiert werden. Beim Import eines Ergebnisses muss erkennbar bleiben, ob es zu den unveränderten Beobachtungen und Rechenparametern gehört.

## 9. Zuordnung der bisherigen Exporte zum Soll

| Bestehender Datenweg | Vorschlag |
| --- | --- |
| KOR-CSV, Digitalisierung, Transformation, Polarpunkte | Gemeinsames KOR-Schema und gemeinsamer Schreiber; Code und unbekannte Höhe erhalten |
| Projektmessdatei | DAT-Kernspalten behalten; Aufstellung und Instrumentenhöhe eindeutig zuordnen |
| Aufnahmetest | KOR mit lokalem Bezug; DAT mit allen Standardspalten und ausdrücklich synthetischem/lokalem Aufstellungskontext, keine erfundenen realen Standpunktdaten |
| `Standpunkt-<Nr>.csv` | Künftig beispielsweise `<Projekt>-<Nr>-<Aufstellung>-stp.csv` |
| `Anschlusspunkte.csv` | Fachlich STP-Arbeitsdatensatz, keine reine KOR-Datei; beispielsweise `<Projekt>-<Aufstellung>-stp.csv`, gemeinsamer Zugriff von Viewer und Stationierung |
| Anschlussdaten vor Festlegung eines Standpunkts | STP-Entwurf mit leerer/noch unbestimmter Standpunktkennung; vor fachlicher Berechnung Zuordnung und Instrumentenhöhe vervollständigen |
| Konvertierung `Punkte.csv` | Je nach Inhalt KOR, DAT oder STP anbieten; gemischte Tabelle anhand tatsächlich vorhandener Daten aufteilen. Herkunfts-`Typ` nach `Quelle`/`PunktTyp` überführen, nicht ungeprüft nach AP/NP |
| Alte `.kor`-/`.dat`-Texte | Als gesonderte Kompatibilitätsformate beibehalten; genaue DAT-Varianten benennen, nicht als identisches Format behandeln |

Bei der Konvertierung müssen fehlende Messwerte von echten Nullmessungen unterscheidbar sein. Die gegenwärtige numerische Tabellenstruktur mit Nullvorgaben benötigt dafür später ebenfalls Anpassungen; allein anhand eines Nullwerts kann nicht entschieden werden, ob eine gültige Beobachtung vorliegt.

### Projektdaten und Tagesprotokoll

Diese Dateien haben andere fachliche Aufgaben als die drei Punkt-/Messdateifamilien. Empfehlung: als ausdrücklich benannte Sonderdateien bestehen lassen, aber dieselben technischen CSV-Regeln verwenden. `Projektdaten.csv` bleibt Parameterhistorie, das Tagesprotokoll bleibt vollständiges Gerätejournal. Beide sollten nicht nur zur Erfüllung eines Suffixschemas in `-kor`, `-dat` oder `-stp` umbenannt werden.

Wenn auch das Gerätejournal zwingend `-dat.csv` heißen soll, wäre `<Projekt>-Data-protocol-YYYY-MM-DD-dat.csv` ein möglicher Name. Dafür wäre ein eigenes ausdrücklich gekennzeichnetes Journalprofil mit allen 28 Feldern nötig; eine Reduktion auf die elf DAT-Kernfelder verlöre Geräteinformationen. Diese weitergehende Vereinheitlichung ist eine separate fachliche Entscheidung.

## 10. Kompatibilität und spätere Umsetzung

Empfohlene Reihenfolge, ohne jetzige Codeänderung:

1. KOR-/DAT-/STP-Spezifikation einschließlich Aufstellungskennung, Präzision und Rolle des Gerätejournals festlegen.
2. Gemeinsame CSV-Grundfunktionen für Maskierung, BOM, Abschnittserkennung, Metadatenerhalt und Fehlerberichte schaffen. Keine Fachparser mit eigenem `split`/`join` daneben belassen.
3. Neue Exporte ausschließlich kanonisch schreiben; alte Komma-/Semikolondateien, sieben-/elfspaltige DAT-Dateien und bisherige Standpunktdateien weiter importieren. Aliase wie R/X, H/Y, Hoehe/Z, HZ/HZ_gon und Punktcode/Code zentral pflegen.
4. Datenmodelle für fehlende Werte, Punktcodes und konkrete Aufstellungen erweitern. Danach Dateneditor, Polarberechnung, Projekt-Nachschlagen, Flächenteilung und Transformation auf Namenszuordnung umstellen.
5. Alle Schreiber anbinden: Digitalisierung, Messablauf, Stationierung, Aufnahmetest, Konvertierung und Polarpunkte. Für STP einen eigenen Datentyp im Editor vorsehen.
6. Dateinamen anhand des gewünschten Datentyps normalisieren; falsches vorhandenes Suffix ersetzen, doppelte Suffixe vermeiden, Groß-/Kleinschreibung beim Erkennen tolerieren. Vorhandene Altdateien nicht ungefragt überschreiben.
7. Beispiele, Anleitung und Dialogfilter aktualisieren. Bei Migration Sicherung und nachvollziehbaren Importbericht erzeugen.

Für spätere Prüfungen sind besonders relevant: Import–Export–Import ohne Verlust von Codes, leeren Höhen und Zusatzfeldern; verschiedene Spaltenreihenfolgen; zitierte Semikola/Anführungszeichen/Zeilenumbrüche; BOM und Dezimalkomma; AP-Mehrfachmessungen; zwei Aufstellungen mit gleicher Standpunktnummer und unterschiedlicher Instrumentenhöhe; Daten nach Stationierungskommentaren; Erhaltung der Polygonreihenfolge in der Flächenteilung.

Die ursprüngliche Analyse basierte auf Quelltext und vorhandenen Beispieldateien. Die anschließende Umsetzung ist nachfolgend beschrieben.

## 11. Umsetzung

Die CSV-Verarbeitung wurde auf das gemeinsame Schema umgestellt:

- `csv_basis.rs` übernimmt Feldmaskierung, mehrzeilige Felder, BOM, Trennzeichenerkennung, Namensaliase und Zahlenformatierung. Doppelte Spaltennamen sowie ungültige Zahlen werden gemeldet.
- `CsvDatei` unterstützt Koordinaten, Messdaten und Standpunktdatensätze, schreibt `Formatversion: 1.0` sowie die kanonischen Kernspalten und erhält unbekannte Metadaten, Zusatzspalten, Kommentare und Stationierungsabschnitte.
- Neue CSV-Ausgaben verwenden die passenden Suffixe `-kor.csv`, `-dat.csv` und `-stp.csv`. Falsche oder doppelte bekannte Suffixe werden beim Normalisieren ersetzt.
- Koordinatenimport und -export, Digitalisierung, Transformation, Flächenteilung, Polarpunkte und Aufnahmetest verwenden die gemeinsame Verarbeitung. Codes und unbekannte Höhen bleiben beim Koordinatenimport erhalten; das interne Importpunktmodell wurde dafür erweitert.
- Projektmessdaten enthalten 13 Kernspalten einschließlich `AufstellungId` und `InstrumentenHoehe_m`. Wiederholte Aufstellungen derselben Nummer werden im Polarmodul getrennt angeboten. Die Kennung wird auch in Stations- und Neupunkt-JSON-Dateien gespeichert, damit Ergebnisse verschiedener Aufstellungen einander nicht ersetzen.
- Die Arbeitsdatei der Anschlussmessungen heißt `<Projekt>-Anschlusspunkte-stp.csv`. `Anschlusspunkte.csv` bleibt als lesbare Altquelle verfügbar, solange keine neue Arbeitsdatei existiert. Gespeicherte Standpunktpakete erhalten Projekt- und Aufstellungskennung im Dateinamen.
- Die freie Stationierung liest AP-Zeilen über Spaltennamen und erhält NP-Zeilen und Zusatzfelder im zugrunde liegenden Dokument. Fehlende Anschlusshöhen werden nicht als bekannte Nullhöhen behandelt. Beim Speichern einer übernommenen Stationierung wird ihr Ergebnis als zusätzlicher Abschnitt ausgegeben.
- Der Datenmanager bietet „Neu STP“ und die Bearbeitung der Standpunktmetadaten. Bei geänderten Beobachtungen oder Instrumentenhöhen verwirft er einen eventuell vorhandenen Ergebnisabschnitt, damit kein veraltetes Ergebnis weitergegeben wird.
- Die Konvertierung bietet getrennte KOR-/DAT-/STP-CSV-Ausgaben. Vorhandensein von Messwerten wird getrennt vom Zahlenwert null geführt. Für DAT/STP muss der Benutzer die Beobachtungen ausdrücklich als AP oder NP kennzeichnen. Unbekannte Standpunktinformationen bleiben leer und müssen vor einer fachlichen Berechnung ergänzt werden.
- Projektdaten schreiben künftig Semikolon-CSV und lesen weiterhin alte Kommadateien. Das Geräte-Tagesprotokoll behält seinen Sondernamen und sämtliche 28 Felder; seine Datenzeilen verwenden die gemeinsame Feldmaskierung.

Kompatibilität: Bisherige Komma-/Semikolontabellen, sieben-/elfspaltige Messdateien, frühere Standpunktdateien und alte KOR-/DAT-Texte bleiben über die jeweiligen Importwege verwendbar. Alte DAT-Koordinatenlisten aus der Konvertierung werden vom Messdaten-DAT-Importer ausdrücklich abgewiesen. Fehlende ursprüngliche Messzeiten werden beim Textimport nicht durch die aktuelle Uhrzeit ersetzt. Vor der Schemaumstellung einer bestehenden Projektmessdatei wird eine Sicherung erstellt. Unbekannte oder beschädigte Messdateiformate werden mit Fehler beibehalten.

Bei Altdateien ohne AP-/NP-Typ wird die bisherige Zuordnung aus vorhandenen Koordinaten weiter unterstützt und im Polarmodul als Hinweis angezeigt. Unbekannte Exzentrizitäten werden ebenfalls ausgewiesen; für die Berechnung gilt zunächst die ausdrücklich gemeldete Annahme null. Reine Richtungsbeobachtungen bleiben im CSV-Dokument erhalten, werden von der vollständige Polarmessungen benötigenden Polarberechnung jedoch als unvollständig gemeldet.

Beispiele und Bedienhinweise: [beispiele/csv/README.md](beispiele/csv/README.md). Die vorhandene `Muster.csv` bleibt zusätzlich als Kompatibilitätsbeispiel erhalten.

Validierung: automatisierte Rust-Tests einschließlich modulübergreifender Prüfungen für Maskierung, mehrzeilige Felder, Spaltenumordnung, Zusatzfelder, fehlende Höhen, AP-Mehrfachmessungen, Aufstellungszuordnung, Ergebnisabschnitte und Exzentrizitätsänderungen. Es wurde kein Test mit angeschlossenem Tachymeter durchgeführt. Ein STP-Ergebnisabschnitt ersetzt weiterhin keine vollständige Projektsicherung mit Rechenparametern, Gewichtungen und Kovarianzen.
