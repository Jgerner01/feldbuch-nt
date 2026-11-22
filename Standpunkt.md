# Standpunkt und Aufstellung – Ist-Stand, Befunde und Umbauvorschlag

Stand: 22.11.2026 · Grundlage: Quelltext Feldbuch-NT 0.6.0 (Arbeitsstand mit Polarpunkt-Umbau) und die Projektdaten „Haidlfing“ vom 29.05.2026.

Dieses Dokument ist die Vorlage für den Umbau. Noch nichts davon ist umgesetzt.

---

## 1. Ziel

1. **Keine automatische Hochzählung** der Standpunktnummer mehr.
2. Ein neuer Standpunkt entsteht **nur ausdrücklich** über den Button **„Neuer Stp“**:
   - im DXF-Viewer ganz links in der Toolbar,
   - in der Freien Stationierung.

   Beide rufen **dieselbe Funktion** auf.
3. Die **Stationierungsinformation** (Standpunkt, Gültigkeit, s0) wandert aus der Toolbar **in die Zeichenfläche, oben links**.
4. Überall gilt **eine** aktive Aufstellung. Kein Programmteil darf eine eigene, abweichende Standpunktnummer verwenden.

---

## 2. Begriffe

| Begriff | Bedeutung im Programm |
|---|---|
| **Standpunktnummer** | Name des Standpunkts, z. B. `STP2`. Bestimmt den Dateinamen `Station-{Nr}.json`. |
| **Aufstellung** | Ein konkretes Aufstellen des Instruments, gekennzeichnet durch die `AufstellungId` = `{Nr}-{Zeitstempel}`. Gleiche Nummer + neue Aufstellung ist möglich (z. B. am nächsten Tag). |
| **Aktive Station** | `Projekt::station` (Datei `Station-{Nr}.json`): Anschlussmessungen, Aktivierung, Ergebnis, Instrumentenhöhe, AufstellungId. |
| **Gültige Stationierung** | `Projekt::hat_gueltige_station()`: konvergiert, ≥ 2 verschiedene Anschlusspunkte. |

---

## 3. Ist-Stand

### 3.1 Wo die Standpunktnummer gespeichert ist

Dieselbe Information steht heute an **sechs** Stellen, die nicht zuverlässig synchron gehalten werden:

| Ort | Datei / Feld | Geschrieben von |
|---|---|---|
| Aktive Station | `Station-{Nr}.json` → `StandpunktNr`, `AufstellungId`, `InstrumentenHoehe` | [projekt.rs](src/daten/projekt.rs) |
| Projektdaten | `Projektdaten.csv` → Kategorie „Freie Stationierung“, Parameter „Standpunkt“ | nur bei **gültiger** Stationierung ([projekt.rs:476-492](src/daten/projekt.rs#L476-L492), [projekt.rs:578-591](src/daten/projekt.rs#L578-L591)) |
| Benutzereinstellungen (**global, nicht je Projekt**) | `%LOCALAPPDATA%\Feldbuch\Einstellungen.xml` → `LetzteStandpunktNr`, `StationierungZaehler` | DXF-Viewer, Freie Stationierung ([einstellungen.rs:53-57](src/daten/einstellungen.rs#L53-L57)) |
| DXF-Viewer | Textfeld „SP“ (`txt_standpunkt`) | [dxf_viewer.rs:422-430](src/gui/dxf_viewer.rs#L422-L430) |
| Freie Stationierung | Textfeld „Standpunktnummer“ (`standpunkt`) | [freie_stationierung.rs:213-219](src/gui/freie_stationierung.rs#L213-L219) |
| Anschlusspunktdatei | `{Projekt}-Anschlusspunkte-stp.csv` → Kopf `StandpunktNr`, `AufstellungId` | [messdaten_eingabe.rs:157-179](src/gui/dxf_viewer/messdaten_eingabe.rs#L157-L179) |

Dazu kommen die Zeilen der Messdatei `{Projekt}-dat.csv` (Spalten `StandpunktNr`, `AufstellungId`, `InstrumentenHoehe_m`) und die Neupunkte `Neupunkte.json` (je Punkt `standpunkt_nr`, `aufstellung_id`).

### 3.2 Automatische Hochzählung und Ersatznummern

| Stelle | Verhalten |
|---|---|
| [freie_stationierung.rs:626-632](src/gui/freie_stationierung.rs#L626-L632) (`nach_dialog`) | **Nach jeder Berechnung**, also schon beim Schließen des Ergebnisdialogs und **vor** der Übernahme: Beginnt die Nummer mit `STP` + Zahl, wird das Feld auf `STP{n+1}` gesetzt, `stationierung_zaehler = n+1`, und über `merke()` landet `STP{n+1}` in `LetzteStandpunktNr`. |
| [freie_stationierung.rs:218](src/gui/freie_stationierung.rs#L218) | Vorbelegung `STP{stationierung_zaehler}`, wenn nichts anderes bekannt ist. |
| [dxf_viewer.rs:427-429](src/gui/dxf_viewer.rs#L427-L429) | Vorbelegung `STP{stationierung_zaehler}` und sofortiges Speichern als `LetzteStandpunktNr`. |
| [messen.rs:23](src/gui/dxf_viewer/messen.rs#L23), [messen.rs:247-250](src/gui/dxf_viewer/messen.rs#L247-L250) | Leeres Feld „SP“ → still `SP01`. |
| [polarpunkte.rs](src/gui/polarpunkte.rs) `STANDPUNKT_STANDARD` | Messung ohne Standpunkt → `SP01`. |
| [csv_datei.rs:26](src/daten/csv_datei.rs#L26) | DAT-Import ohne Standpunkt → `STP`. |
| [konvertierung.rs:17](src/daten/konvertierung.rs#L17) | DAT-Export ohne Standpunkt → `STP1`. |

Es gibt also **drei verschiedene Ersatznummern** (`SP01`, `STP`, `STP1`) und einen Zähler, der an der falschen Stelle hochzählt.

### 3.3 Wie heute ein Standpunkt entsteht (vier Wege)

| Weg | Auslöser | Was passiert |
|---|---|---|
| A | Freie Stationierung → „Neuer Standpunkt“ ([freie_stationierung.rs:355-380](src/gui/freie_stationierung.rs#L355-L380)) | Anschlusspunktdatei archivieren, `Projekt::neue_aufstellung(nr, ih)`: alte Station archivieren, neue AufstellungId, `Station-{Nr}.json` schreiben. Der Button erscheint **nur, wenn die Anschlusspunktdatei existiert** ([freie_stationierung.rs:791](src/gui/freie_stationierung.rs#L791)). |
| B | DXF-Viewer: Feld „SP“ verlassen ([zeichnen.rs:153-164](src/gui/dxf_viewer/zeichnen.rs#L153-L164)) | `station_laden(nr)`: vorhandene Datei laden **oder** stillschweigend eine leere Station im Speicher anlegen (nicht gespeichert). Eine alte Station gleicher Nummer wird **ohne Rückfrage** wieder aktiv. |
| C | DXF-Viewer: Live-Messung im Stationierungsmodus ([messen.rs:368-375](src/gui/dxf_viewer/messen.rs#L368-L375)) | Weicht die Feldnummer von der aktiven Station ab: `Projekt::neuer_standpunkt` (lädt/erzeugt ohne Archiv). |
| D | Freie Stationierung → Berechnen → Speichern ([projekt.rs:520-548](src/daten/projekt.rs#L520-L548)) | Mit der Nummer aus dem Feld. Gibt es die Nummer noch nicht oder weicht sie ab: **neue AufstellungId**, danach überschrieben durch die Id aus der Anschlusspunktdatei. |

Dazu entsteht eine **neue Aufstellung implizit**, sobald sich die Instrumentenhöhe um mehr als 0,5 mm ändert ([projekt.rs:310-312](src/daten/projekt.rs#L310-L312)). Das passiert bei jeder Eingabe im Feld „IH“ des DXF-Viewers und bei jeder Live-Anschlussmessung.

### 3.4 Wie die Messungen zum Standpunkt kommen

| Messung | Standpunktnummer kommt aus | Stationierung zur Berechnung kommt aus |
|---|---|---|
| Anschlusspunkt im DXF-Viewer (Dialog „identischer Punkt“) → Anschlusspunktdatei | `projekt.station.standpunkt_nr` (aktive Station, **nicht** das Feld „SP“) | – |
| Neupunkt im DXF-Viewer → dat.csv, Neupunkte.json | Feld **„SP“** (`txt_standpunkt`) | `projekt.station` (**aktive Station**) |
| Freie Stationierung | Feld „Standpunktnummer“, wird beim Laden der Anschlusspunktdatei **überschrieben** | eigene Berechnung |
| Polarpunkte | Gruppen aus der dat.csv | Station-Datei, dat.csv oder stp.csv |
| Absteckung ([absteckung.rs:96-105](src/rechnen/absteckung.rs#L96-L105)) | Projektdaten, **zuerst Rückwärtsschnitt, dann Freie Stationierung** | Projektdaten |

---

## 4. Befunde

Schwere: **A** = falsche Zuordnung von Messdaten möglich, **B** = Bedienung irreführend / Daten inkonsistent, **C** = Schönheitsfehler.

| Nr. | Schwere | Befund | Fundstelle |
|---|---|---|---|
| B1 | **A** | **Hochzählung vor der Übernahme.** Schon das Schließen des Ergebnisdialogs setzt die Nummer auf `STP{n+1}`. Wird danach erneut gerechnet und gespeichert, entsteht eine zweite Station `STP{n+1}` mit denselben Anschlussmessungen und derselben AufstellungId. `LetzteStandpunktNr` zeigt dann auf eine Nummer, die es physisch nicht gibt. | [freie_stationierung.rs:626-633](src/gui/freie_stationierung.rs#L626-L633) |
| B2 | **A** | **Neupunkte: Nummer und Stationierung aus verschiedenen Quellen.** Die Nummer kommt aus dem Feld „SP“, die Koordinaten aus `projekt.station`. Weichen beide ab, werden Punkte mit der Stationierung von STP1 berechnet, aber unter STP2 gespeichert. | [messen.rs:247-314](src/gui/dxf_viewer/messen.rs#L247-L314) |
| B3 | **A** | **Folge von B2:** Wird die Station später neu übernommen, rechnet `berechne_neu` nur die Neupunkte mit **gleicher** Standpunktnummer neu. Falsch zugeordnete Punkte behalten stillschweigend alte Koordinaten. | [punktlisten.rs:323-357](src/daten/punktlisten.rs#L323-L357) |
| B4 | **A** | **Feld „SP“ wird nicht nachgeführt.** Beim Zurückkehren in den DXF-Viewer lädt `bei_aktivierung` die Station aus den Projektdaten neu, das Feld „SP“ behält aber den alten Text. | [dxf_viewer.rs:956-962](src/gui/dxf_viewer.rs#L956-L962) |
| B5 | **A** | **AufstellungId mit leerer Nummer.** Ohne aktive Station schreibt der DXF-Viewer die Anschlusspunktdatei mit leerer `StandpunktNr`; `aufstellung_id(…, "", …)` erzeugt dann `-{Zeitstempel}`. Diese Id wird bei der Übernahme in die Station kopiert und steht anschließend unter jeder Messung. | [messdaten_eingabe.rs:162-175](src/gui/dxf_viewer/messdaten_eingabe.rs#L162-L175), [messdaten.rs:145-148](src/daten/messdaten.rs#L145-L148) |
| B6 | **A** | **AufstellungId wird bei jedem Anschlusspunkt neu bestimmt** (`set_meta` bei jedem Schreiben). Ohne Station-Datei entsteht jedes Mal eine neue Id. | [messdaten_eingabe.rs:175](src/gui/dxf_viewer/messdaten_eingabe.rs#L175) |
| B7 | **A** | **Instrumentenhöhe erzeugt still eine neue Aufstellung.** Jede Änderung > 0,5 mm (auch eine Tippfehler-Korrektur) vergibt eine neue Id. Danach passen Neupunkte und Station nicht mehr zusammen, und `berechne_neu` erfasst die älteren Punkte nicht mehr. | [projekt.rs:304-315](src/daten/projekt.rs#L304-L315) |
| B8 | B | **Feld „SP“ lädt beim Verlassen eine alte Station** gleicher Nummer ohne Rückfrage (auch eine vom Vortag) und überschreibt damit die aktive Stationierung im Speicher. | [zeichnen.rs:153-164](src/gui/dxf_viewer/zeichnen.rs#L153-L164) |
| B9 | B | **Nummer springt in der Freien Stationierung.** Schreibt der DXF-Viewer einen Anschlusspunkt, lädt die Freie Stationierung die Datei neu und übernimmt deren Kopfnummer ins Feld; eine dort eingetippte Nummer geht verloren. | [freie_stationierung.rs:274-280](src/gui/freie_stationierung.rs#L274-L280), [freie_stationierung.rs:433-435](src/gui/freie_stationierung.rs#L433-L435) |
| B10 | B | **Zähler und letzte Nummer sind global** (Einstellungen.xml im Benutzerordner), nicht je Projekt. Beim Öffnen eines vorhandenen Projekts gilt die Nummer des zuletzt bearbeiteten anderen Projekts. Nur `neues_leeres_projekt` setzt zurück. | [einstellungen.rs:188-192](src/daten/einstellungen.rs#L188-L192) |
| B11 | B | **Vier Wege, einen Standpunkt anzulegen** (Abschnitt 3.3), mit unterschiedlichem Verhalten bei Archiv, AufstellungId und Speichern. | – |
| B12 | B | **„Neuer Standpunkt“ fehlt manchmal.** In der Freien Stationierung erscheint der Button nur, wenn die Anschlusspunktdatei existiert; im DXF-Viewer gibt es keinen. | [freie_stationierung.rs:791-797](src/gui/freie_stationierung.rs#L791-L797) |
| B13 | B | **Absteckung nimmt immer zuerst den Rückwärtsschnitt**, auch wenn danach frei stationiert wurde, und nicht die aktive Station. | [absteckung.rs:96-105](src/rechnen/absteckung.rs#L96-L105) |
| B14 | B | **Zwei Pfade für die Stationierung im DXF-Viewer**: Dialog → Anschlusspunktdatei (ohne dat.csv-Eintrag) und Live-Pfad `verarbeite_stationierung` (dat.csv + direkte Berechnung). Der Live-Pfad wird nur erreicht, wenn im Stationierungsmodus ohne offenen Dialog ein Messergebnis eintrifft. | [messen.rs:180](src/gui/dxf_viewer/messen.rs#L180), [messen.rs:355-420](src/gui/dxf_viewer/messen.rs#L355-L420) |
| B15 | C | Drei verschiedene Ersatznummern (`SP01`, `STP`, `STP1`). | Abschnitt 3.2 |
| B16 | C | Gültigkeit der Stationierung steht nur als kleine Lampe mit Tooltip in der Toolbar; die Standpunktnummer selbst ist im Feld „SP“ nicht von „nur eingetippt“ zu unterscheiden. | [dxf_viewer.rs:1113-1125](src/gui/dxf_viewer.rs#L1113-L1125) |

### 4.1 Nachweis an den Haidlfing-Daten (29.05.2026)

Die Projektdateien zeigen genau das Zusammenspiel von B1, B2 und B5:

- **Projektdaten.csv:** 22:25 Freie Stationierung **STP1** übernommen (R 772569.346 / H 5405060.549), 22:31 Freie Stationierung **STP2** übernommen mit praktisch gleichen Koordinaten. Das ist **eine** Aufstellung, zweimal unter verschiedenen Nummern gespeichert (B1).
- **AufstellungId `-20260529T222346435520600`:** Sie beginnt mit `-`, wurde also mit **leerer** Standpunktnummer erzeugt (B5). Sie steht in der stp.csv **und** unter allen Messungen von STP1 und STP2.
- **Haidlfing-dat.csv:** Punkt 11 steht unter STP1, alle weiteren unter STP2, alle mit derselben AufstellungId (B1/B2). Genau diese Kombination hat den Polarpunkt-Leser abbrechen lassen (inzwischen nur noch ein Hinweis).

---

## 5. Vorschlag

### 5.1 Grundregeln

1. **Eine Quelle:** Die aktive Aufstellung ist `Projekt::station`. Alle Fenster **zeigen** sie an, keines führt eine eigene Nummer.
2. **Ein Weg:** Ein neuer Standpunkt bzw. eine neue Aufstellung entsteht **nur** über `Projekt::standpunkt_beginnen(…)` (neu, ersetzt `neuer_standpunkt` und `neue_aufstellung`), ausgelöst durch den Dialog „Neuer Stp“.
3. **Keine Ersatznummern:** Ohne aktive Aufstellung wird nicht gemessen. Statt `SP01` erscheint der Hinweis *„Kein Standpunkt – bitte ‚Neuer Stp‘“*.
4. **Keine stillen Seiteneffekte:** Weder ein Textfeld noch die Instrumentenhöhe noch eine Berechnung erzeugt nebenbei einen Standpunkt oder eine Aufstellung.
5. **Je Projekt:** Die zuletzt aktive Aufstellung wird in `Projektdaten.csv` des Projekts gespeichert, nicht in den globalen Einstellungen.

### 5.2 Dialog „Neuer Stp“ (gemeinsam für DXF-Viewer und Freie Stationierung)

Neue Datei `src/gui/standpunkt_dialog.rs`, aufgerufen aus beiden Fenstern.

```
┌─ Neuer Standpunkt ─────────────────────────────────┐
│ Standpunktnummer:   [ STP3        ]                │
│ Instrumentenhöhe:   [ 1.600 ] m                    │
│ Code (optional):    [             ]                │
│                                                    │
│ Vorhandene Standpunkte im Projekt:                 │
│   STP1   29.05.2026 22:25   s0 1.9 mm   11 AP      │
│   STP2   29.05.2026 22:31   s0 1.9 mm   11 AP      │
│                                                    │
│ ⚠ STP2 existiert bereits (29.05.2026).             │
│   ( ) Neue Aufstellung unter gleicher Nummer       │
│       (bisherige Station wird archiviert)          │
│   ( ) Andere Nummer wählen                         │
│                                                    │
│               [ Abbrechen ]   [ Standpunkt anlegen ]│
└────────────────────────────────────────────────────┘
```

- **Nummernvorschlag (Entscheidung F1):** Jedes Projekt beginnt mit **STP1**. Bei „Neuer Stp“ schlägt der Dialog die nächste Nummer vor: höchstes `STPn` im **Projekt** + 1. Gezählt werden `Station-*.json`, die Spalte `StandpunktNr` der dat.csv und Feldbuchpunkte vom Typ Standpunkt. Es gibt keinen gespeicherten Zähler, damit Importe und Archive mitzählen. Der Vorschlag ist editierbar und gilt erst mit „Standpunkt anlegen“. Gezählt wird **nur hier**, nie nach einer Berechnung.
- Prüfungen: Nummer nicht leer, keine Zeichen `\/:*?"<>|`, Instrumentenhöhe ≥ 0.
- Existiert `Station-{Nr}.json`: Auswahl **neue Aufstellung (Archiv)** oder **andere Nummer**. Das bloße Weiterarbeiten an einer alten Station läuft über „Standpunkt fortsetzen“ (Abschnitt 5.6), nicht über diesen Dialog.

**`Projekt::standpunkt_beginnen(nr, ih, code)`** erledigt in **einem** Schritt:

1. bisherige `Station-{Nr}.json` gleicher Nummer archivieren (wie heute `neue_aufstellung`),
2. neue AufstellungId `{Nr}-{Zeitstempel}` erzeugen,
3. `Station-{Nr}.json` leer mit Nr, IH, Id, Datum speichern,
4. Anschlusspunktdatei archivieren und mit Kopf (Nr, IH, **dieselbe Id**) neu anlegen,
5. in `Projektdaten.csv` die aktive Aufstellung vermerken (neue Kategorie „Aktiver Standpunkt“: Nr, AufstellungId, IH),
6. `station_version` erhöhen, damit alle offenen Fenster neu zeichnen,
7. Protokolleintrag `STANDPKT: Neuer Standpunkt STP3, IH 1.600 m, Id …`.

### 5.3 DXF-Viewer

**Toolbar** ([dxf_viewer.rs:1062-1192](src/gui/dxf_viewer.rs#L1062-L1192)):

```
heute:  [Prisma][P][●][PDF][DXF] [Modus] … | ● s0 | IH [1.600] | SP [STP2] | ► Messen | ZH | Nr | Code
neu:    [Neuer Stp][Prisma][P][●][PDF][DXF] [Modus] … | IH [1.600] | ► Messen | ZH | Nr | Code
```

- **„Neuer Stp“** ganz links, auffällig (Farbe wie heute „Neuer Standpunkt“ in der Freien Stationierung), öffnet den Dialog 5.2.
- Die anderen linken Buttons rücken um eine Position nach rechts.
- **Feld „SP“ entfällt** (die Nummer steht in der Zeichenfläche). Damit entfallen B4 und B8.
- **Lampe und s0-Text entfallen** in der Toolbar (wandern in die Zeichenfläche).
- **Feld „IH“ bleibt**, wirkt aber als **Korrektur derselben Aufstellung** (keine neue AufstellungId). Bei Änderung > 0,5 mm erscheint eine Rückfrage: *„Instrumentenhöhe für STP2 von 1.600 auf 1.650 korrigieren? Vorhandene Neupunkte dieser Aufstellung werden neu berechnet.“* Siehe Frage F2.

**Zeichenfläche oben links** (neu, gezeichnet in `zeichne_canvas` nach den Daten, vor dem Fadenkreuz):

```
┌──────────────────────────────────────┐
│ ● STP2   IH 1.600 m                  │   ← Lampe grün/gelb/rot
│   Gute Stationierung  s0 1.9 mm r 19 │
│   Aufstellung 29.05.2026 22:23       │
└──────────────────────────────────────┘
```

| Zustand | Anzeige |
|---|---|
| Keine aktive Aufstellung | grau/rot: **„Kein Standpunkt – ‚Neuer Stp‘ drücken“** |
| Aufstellung angelegt, noch < 2 AP | gelb: „STP3 – Stationierung offen (1 von 2 Anschlusspunkten)“ |
| Gültig | Lampe nach s0-Schwellen wie heute ([dxf_viewer.rs:615-633](src/gui/dxf_viewer.rs#L615-L633)) |
| Lokales System | „Lokales System 0/0/0“ (heute unten rechts in der Statusleiste) |

Halbtransparenter Hintergrund, damit die Zeichnung lesbar bleibt. Klick auf das Feld öffnet die Stationsübersicht (optional, siehe F4).

**Messen** ([messen.rs](src/gui/dxf_viewer/messen.rs)):

- Standpunktnummer, AufstellungId und IH kommen **immer** aus `projekt.station`.
- Ohne aktive Aufstellung: keine Messung, Hinweis in der Zeichenfläche und Statuszeile, Fehlerton.
- Neupunkte ohne gültige Stationierung werden wie heute als Rohdaten gespeichert, jetzt aber garantiert mit der richtigen Nummer und Id. Sie werden dann bei der Übernahme der Stationierung durch `berechne_neu` erfasst.
- `schreibe_anschlusspunkt` übernimmt Nummer und Id aus der Station und berechnet sie **nicht** bei jedem Schreiben neu (B5, B6).
- Live-Pfad `verarbeite_stationierung` (B14): entfernen oder ebenfalls über die Anschlusspunktdatei führen, siehe F3.

### 5.4 Freie Stationierung

- **„Neuer Standpunkt“ → „Neuer Stp“**, immer sichtbar, ruft denselben Dialog 5.2 auf.
- Feld „Standpunktnummer“ wird **Anzeige** der aktiven Aufstellung (nicht editierbar).
- **Hochzählung in `nach_dialog` entfällt** vollständig ([freie_stationierung.rs:626-632](src/gui/freie_stationierung.rs#L626-L632)), ebenso die Vorbelegung `STP{Zähler}`.
- „CSV laden …“ mit einer Datei, deren Standpunkt/Id von der aktiven Aufstellung abweicht → Rückfrage:
  - *„Als neuen Standpunkt STPx anlegen“* (Dialog 5.2 vorbelegt), oder
  - *„Messungen der aktiven Aufstellung STP2 zuordnen“*, oder Abbrechen.
- Das automatische Nachladen der Anschlusspunktdatei ändert **nie** die Nummer (B9).
- Speichern/Übernahme nur in die **aktive** Aufstellung. Passt sie nicht zur Datei: Hinweis statt stiller Neuanlage.

### 5.5 Weitere Module

| Modul | Änderung |
|---|---|
| [projekt.rs](src/daten/projekt.rs) | `standpunkt_beginnen` neu; `neuer_standpunkt` / `neue_aufstellung` darin aufgehen lassen; `set_instrumenten_hoehe` ohne neue Id; `set_ergebnis_manuell` erzeugt keine Id mehr selbst; `lade_aktuellen_standpunkt` liest die neue Kategorie „Aktiver Standpunkt“ (Rückfall: „Freie Stationierung/Standpunkt“). |
| [messdaten.rs](src/daten/messdaten.rs) | `aufstellung_id(e, nr, ih)` entfällt bzw. wird nur noch für Altdaten gebraucht; Schreiber bekommen die Id aus der Station. `neue_aufstellung_id` lehnt leere Nummern ab. |
| [einstellungen.rs](src/daten/einstellungen.rs) | `StationierungZaehler` und `LetzteStandpunktNr` nicht mehr verwenden (beim Lesen ignorieren, nicht mehr schreiben). |
| [absteckung.rs:96-105](src/rechnen/absteckung.rs#L96-L105) | Standpunkt der **aktiven Station** bevorzugen; Rückwärtsschnitt nur, wenn er jünger ist oder ausdrücklich gewählt wurde (F5). |
| [polarpunkte.rs](src/gui/polarpunkte.rs) | `SP01`-Ersatz entfernen; Messungen ohne Standpunkt als Gruppe „(ohne Standpunkt)“ anzeigen. |
| [csv_datei.rs](src/daten/csv_datei.rs), [konvertierung.rs](src/daten/konvertierung.rs) | Import/Export: Ersatznummer nur, wenn der Benutzer sie im Konvertierungsdialog bestätigt; einheitlich `STP1`. |

### 5.6 Standpunkt fortsetzen (Wiederaufnahme)

Heute geschieht das versteckt über das Feld „SP“. Vorschlag: im Dialog „Neuer Stp“ eine zweite Schaltfläche **„Vorhandenen fortsetzen“** (Liste der Stationen des Projekts). Sie lädt `Station-{Nr}.json` samt AufstellungId und macht sie aktiv, **ohne** neue Id. Typischer Fall: Programm neu gestartet, Instrument steht noch.

---

## 6. Umsetzungsschritte

| Schritt | Inhalt | Prüfung |
|---|---|---|
| 1 | `Projekt::standpunkt_beginnen` + `standpunkt_fortsetzen`, Kategorie „Aktiver Standpunkt“, Tests | Unit-Tests in `projekt.rs` |
| 2 | Dialog `standpunkt_dialog.rs` | manuell |
| 3 | DXF-Viewer: Button „Neuer Stp“, Feld „SP“ und Lampe raus, Infofeld in der Zeichenfläche | manuell, Toughpad-Breite |
| 4 | DXF-Viewer: Messen nur mit aktiver Aufstellung, Nummer/Id aus der Station, `schreibe_anschlusspunkt` korrigieren | Unit-Test für `schreibe_anschlusspunkt` |
| 5 | Freie Stationierung: Hochzählung raus, Feld als Anzeige, „Neuer Stp“, CSV-Rückfrage | manuell |
| 6 | IH-Korrektur ohne neue Aufstellung, mit Neuberechnung der Neupunkte | Unit-Test |
| 7 | Einstellungen entrümpeln, Ersatznummern vereinheitlichen, Absteckung | Unit-Tests |
| 8 | Live-Pfad `verarbeite_stationierung` je nach F3 | – |

### 6.1 Testfälle (Abnahme)

1. Neues Projekt → DXF-Viewer: Infofeld „Kein Standpunkt“, Messen gesperrt.
2. „Neuer Stp“ STP1, IH 1.600 → Station-STP1.json, Anschlusspunktdatei und Projektdaten haben **dieselbe** Id `STP1-…`.
3. Drei Anschlusspunkte im DXF-Viewer, Freie Stationierung rechnen, Ergebnisdialog schließen, **zweimal** rechnen, speichern → genau **eine** Station STP1, das Feld bleibt STP1.
4. Neupunkte messen → dat.csv-Zeilen und Neupunkte.json mit STP1 und derselben Id.
5. IH von 1.600 auf 1.650 korrigieren → Rückfrage, Id bleibt, Neupunkte neu berechnet.
6. „Neuer Stp“ STP1 erneut → Rückfrage „existiert“, neue Aufstellung → alte Station archiviert, neue Id.
7. Programm neu starten, anderes Projekt öffnen → dessen aktive Aufstellung erscheint, nicht die des vorherigen Projekts.
8. Freie Stationierung, „CSV laden“ mit fremder stp.csv → Rückfrage, keine stille Umbenennung.
9. Polarpunkte mit den Haidlfing-Daten → weiterhin lesbar (Altdaten).

---

## 7. Altdaten

- Vorhandene `Station-*.json`, dat.csv und Neupunkte.json bleiben **unverändert** lesbar.
- Ids der Form `-{Zeitstempel}` (leere Nummer) werden beim Lesen akzeptiert. Beim Fortsetzen einer solchen Station schlägt das Programm vor, eine neue Aufstellung anzulegen.
- Für Haidlfing: STP1 und STP2 sind dieselbe Aufstellung. Eine Bereinigung (Punkt 11 → STP2 oder Station STP1 löschen) sollte bewusst von Hand im Daten-Editor erfolgen, nicht automatisch.

---

## 8. Entscheidungen (22.11.2026)

| Nr. | Frage | Entscheidung |
|---|---|---|
| F1 | Nummernvorschlag im Dialog „Neuer Stp“ | **Jedes Projekt beginnt mit STP1**; „Neuer Stp“ schlägt die nächste Nummer vor (höchstes `STPn` im Projekt + 1), editierbar, wirksam erst nach Bestätigung. |
| F2 | Feld „IH“ im DXF-Viewer | **Bleibt**, wirkt als Korrektur derselben Aufstellung, mit Rückfrage. |
| F3 | Live-Stationierung `verarbeite_stationierung` | **Entfällt.** |
| F4 | Klick auf das Infofeld öffnet Stationsübersicht | offen, später |
| F5 | Absteckung | **Aktive Station als Vorgabe**, Rückwärtsschnitt nur per Auswahl. |
| F6 | Lokales System | **Auch dort zuerst „Neuer Stp“.** |

---

## 9. Weitere Vorschläge

Diese Punkte gehen über die ursprüngliche Anfrage hinaus. Sie sind nach Nutzen sortiert; über jeden wird vor der Umsetzung einzeln entschieden.

### V1 Aufstellung beim Programmstart prüfen

Heute lädt der DXF-Viewer beim Start stillschweigend die zuletzt aktive Station, auch wenn sie von einem anderen Tag stammt. Wer am nächsten Morgen sofort misst, ordnet die Punkte der Aufstellung von gestern zu.

**Vorschlag:** Ist die aktive Aufstellung nicht von heute (oder älter als z. B. 8 h), erscheint beim Öffnen des DXF-Viewers:
*„Aktive Aufstellung STP2 vom 29.05.2026, 22:23. Steht das Instrument noch unverändert? [Fortsetzen] [Neuer Stp]“*

### V2 Orientierungskontrolle beim Fortsetzen

Ein Instrument kann angestoßen werden oder einsinken. Beim **Fortsetzen** einer Aufstellung (V1, Abschnitt 5.6) und auf Wunsch jederzeit:
- einen Anschlusspunkt der Station anzielen und messen,
- Abweichung in Richtung (mgon bzw. cc und mm quer) und Strecke anzeigen,
- Ampel nach den Fehlergrenzen der Rechenparameter; bei Rot den Hinweis „Neu stationieren“.

Das Ergebnis kommt als Kontrollzeile ins Protokoll und in die dat.csv (Typ `KP`, Kontrollpunkt). Die Stationierung selbst ändert sich dadurch nicht.

### V3 Warnung bei offenen Neupunkten

Drückt man „Neuer Stp“, während die aktive Aufstellung **Neupunkte ohne gültige Stationierung** hat, gehen diese leicht vergessen (sie haben nur Rohdaten).

**Vorschlag:** Rückfrage *„STP2 hat 12 Neupunkte ohne Stationierung. Trotzdem neuen Standpunkt beginnen?“* Später kann man sie über die Polarpunktberechnung (stp.csv laden) noch auswerten.

### V4 Standpunkt umbenennen / zusammenführen

Für Fälle wie Haidlfing (eine Aufstellung unter STP1 **und** STP2) fehlt heute ein sauberes Werkzeug. Von Hand müsste man mehrere Dateien ändern.

**Vorschlag:** Funktion „Standpunkt umbenennen“ (z. B. im Daten-Editor oder in der Stationsübersicht). Sie ändert in einem Schritt, mit Sicherung vorher:
- `Station-{alt}.json` → `Station-{neu}.json`
- Zeilen der dat.csv (`StandpunktNr`), Neupunkte.json, Feldbuchpunkt vom Typ Standpunkt, Projektdaten

Variante **„Zusammenführen“**: Zwei Nummern mit derselben AufstellungId werden zu einer.

### V5 Bekannter Standpunkt (Aufstellung über Festpunkt)

Heute gibt es nur die freie Stationierung (≥ 2 Anschlusspunkte). Steht das Instrument auf einem **koordinatenmäßig bekannten Punkt**, reicht ein Anschlussziel. Die Rechenfunktion dafür gibt es bereits (`polar::orientierung_aus_einem_ziel`); sie wird bisher nur im Polarpunkte-Fenster verwendet.

**Vorschlag:** Im Dialog „Neuer Stp“ ein Kontrollkästchen *„Standpunkt ist bekannter Punkt“* mit Auswahl aus Feldbuch-/Importpunkten. Die Stationierung gilt dann ab dem ersten Anschlussziel. Weitere Ziele dienen der Kontrolle (Orientierungsmittel).

### V6 Instrumentenhöhe plausibilisieren

In Haidlfing wurde mit **IH = 0.000** gemessen. Bei 3D-Berechnung sind die Höhen der Neupunkte dann um die Instrumentenhöhe falsch.

**Vorschlag:** Hinweis im Dialog „Neuer Stp“ und in der Zeichenfläche, wenn 3D-Berechnung eingeschaltet ist und IH = 0 oder unplausibel (< 0,8 m bzw. > 2,2 m bei Stativaufstellung). Nur ein Hinweis, keine Sperre.

### V7 Gerätezustand mit der Aufstellung speichern

Bei „Neuer Stp“ automatisch in `Station-{Nr}.json` ablegen:
- Gerät und Seriennummer (falls verbunden),
- Prismenkonstante und EDM-Modus,
- Atmosphäre (ppm), Längs-/Querneigung aus der Libelle.

Das gehört ins Stationierungsprotokoll und hilft später bei der Fehlersuche, etwa wenn die Prismenkonstante während der Aufstellung umgestellt wurde.

### V8 Nummernkonflikt Standpunkt ↔ Punktnummer

`Projekt::finde_anschlusspunkt` nimmt den **ersten** Feldbuchpunkt mit passender Nummer, **egal welchen Typs** ([projekt.rs:241](src/daten/projekt.rs#L241)). Heißt ein Standpunkt so wie ein Anschlusspunkt (z. B. „100“), können dessen Koordinaten versehentlich als Anschlusskoordinaten verwendet werden.

**Vorschlag:**
- Im Dialog „Neuer Stp“ warnen, wenn die Nummer schon als Punktnummer existiert.
- `finde_anschlusspunkt` ignoriert Feldbuchpunkte vom Typ Standpunkt.

### V9 Standpunktwechsel in den Protokollen sichtbar machen

Das Tagesprotokoll `Data-protocol-{Datum}.csv` enthält den Standpunkt nur je Zeile. Ein Wechsel ist beim Lesen leicht zu übersehen.

**Vorschlag:** Bei „Neuer Stp“ eine Kommentarzeile `# NEUER STANDPUNKT: STP3  IH=1.600 m  Id=STP3-…` ins Tagesprotokoll und in die dat.csv schreiben. Die dat.csv kennt solche Kommentarzeilen schon (`# STATIONIERUNG:`).

### V10 Standpunkt im Plan mit Zielstrahlen

Die Zeichenfläche zeigt den Standpunkt bereits als Punkt. Auf Wunsch (Ein/Aus wie „Residuen“) dünne Linien vom Standpunkt zu allen Anschlusspunkten der aktiven Aufstellung, farbig nach Residuum. Damit fällt eine schlechte Geometrie (alle Ziele in einer Richtung) sofort auf.

---

## 10. Umsetzungsstand (22.11.2026)

Alle Punkte aus Abschnitt 5, die Entscheidungen F1–F3, F5, F6 sowie V1–V10 sind umgesetzt. F4 (Klick auf das Infofeld) ist offen.

| Punkt | Umsetzung |
|---|---|
| Kern | `Projekt::standpunkt_beginnen`, `standpunkt_fortsetzen`, `standpunkt_umbenennen`, `naechste_standpunkt_nr`, `standpunkte_uebersicht`, `hat_aktive_aufstellung`, `berechne_station` in [projekt.rs](src/daten/projekt.rs); aktive Aufstellung je Projekt in `Projektdaten.csv` (Kategorie „Aktiver Standpunkt“) |
| F1 | Vorschlag STP1 bzw. höchste `STPn` + 1 aus Stationsdateien, Messdatei und Feldbuchpunkten; `StationierungZaehler` und `LetzteStandpunktNr` aus den Einstellungen entfernt |
| F2 | Feld „IH“ korrigiert die aktive Aufstellung nach Rückfrage, ohne neue Id; Neupunkte werden neu berechnet |
| F3 | Live-Stationierung im DXF-Viewer entfernt; Anschlusspunkte nur über den identischen Punkt |
| F5 | Absteckung: aktive Aufstellung als Vorgabe, Rückwärtsschnitt per Auswahl |
| F6 | Ohne aktive Aufstellung wird nicht gemessen (auch im lokalen System) |
| Dialog | [standpunkt_dialog.rs](src/gui/standpunkt_dialog.rs): Neu / Fortsetzen / Umbenennen, gemeinsam für DXF-Viewer und Freie Stationierung |
| DXF-Viewer | „Neuer Stp“ und „Kontrolle“ links in der Toolbar; Feld „SP“ und Lampe entfernt; Infofeld oben links ([dxf_viewer/standpunkt.rs](src/gui/dxf_viewer/standpunkt.rs)) |
| Freie Stationierung | Keine Hochzählung; Standpunkt nur Anzeige; „Neuer Stp“ in der Kopfzeile; Rückfrage bei CSV eines anderen Standpunkts |
| V1 | Rückfrage beim Öffnen des DXF-Viewers bei Aufstellungen von einem anderen Tag oder älter als 8 h |
| V2 | Orientierungskontrolle (Toolbar „Kontrolle“, automatisch nach „Fortsetzen“); Zeile Typ `KP` in der Messdatei |
| V3 | Rückfrage bei „Neuer Stp“, wenn Neupunkte ohne Stationierung offen sind; diese Rohdaten werden bei der Übernahme berechnet |
| V4 | Umbenennen / Zusammenführen mit Sicherung in `Sicherung/` |
| V5 | Bekannter Standpunkt: Orientierungsmittel (`polar::station_bekannter_punkt`), ein Ziel genügt |
| V6 | Hinweis bei IH 0 bzw. außerhalb 0,8–2,2 m (bei 3D) im Dialog, im Infofeld und in der Freien Stationierung |
| V7 | Gerätezustand in `Station-{Nr}.json` (Feld `Geraet`) und im Protokoll |
| V8 | `finde_anschlusspunkt` ignoriert Standpunkte; Warnung bei Nummernkonflikt |
| V9 | Kommentarzeile `# NEUER STANDPUNKT` / `# STANDPUNKT FORTGESETZT` in Messdatei und Tagesprotokoll; `# STATIONIERUNG` nach jeder Übernahme |
| V10 | Zielstrahlen (Statusleiste „Zielstrahlen“) |
| Ersatznummern | Polarpunkte: Gruppe „(ohne Standpunkt)“, Speichern gesperrt; DAT-Export ohne Standpunkt einheitlich `STP1` |
