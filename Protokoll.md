# Messprotokoll – Prüfung und Verbesserungsvorschlag

Stand: 22.11.2026 · Status: **Schritte 1–8 umgesetzt (Abschnitt 6), Schritt 9 offen**

Ziel: ein lückenloses Messprotokoll, das jede Messung und jede Änderung festhält (Messwerte, Prismentyp/EDM,
Instrumenten- und Zielhöhe, Punktnummern, Korrekturen, Löschungen …). Aus diesem Protokoll soll später eine neue
Funktion die Dateien `stp.csv` (Standpunkte/Anschlusspunkte), `dat.csv` (Messdaten) und ggf. Koordinaten
**wiederherstellen** können.

---

## 1. Ist-Zustand

Heute gibt es vier Stellen, an die Mess- oder Ereignisdaten geschrieben werden:

| Datei | Erzeugt in | Inhalt | Eignung als Grundlage |
|---|---|---|---|
| `Data-protocol-JJJJ-MM-TT.csv` („Tagesprotokoll“) | `daten/messdaten.rs` `schreibe_tagesprotokoll` | Eine Zeile pro Messung: Zeit, StationNr, IH, PunktNr, Code, Methode, EDM, Prismenkonstante, ppm, Druck, Temp, Neigungen, Hz, V, S, Zielhöhe, Quelle, GNSS (leer). Dazu Kommentarzeilen bei Standpunktwechsel. Nur Anhängen. | Gute Basis, aber unvollständig (siehe 2.) |
| `Protokoll_JJJJ-MM-TT.txt` | `grundlagen/protokoll.rs` `log` | Freitext-Ereignisse (STANDPKT, RECHNUNG, POLAR, LÖSCH, KONFIG, TACHY …) | **Ungeeignet**: abschaltbar (Einstellung „Protokollierung“), Freitext, nicht maschinenlesbar |
| `{Projekt}-dat.csv` | `messdaten.rs` `schreibe_messung` | NP- und KP-Zeilen mit AufstellungId, Exzentrizitäten, Kommentar „# STATIONIERUNG …“ | Ergebnisdatei, wird nachträglich geändert (Exzentrizitäten, Datenmanager) → kein Protokoll |
| Anschlusspunktdatei (`stp`-CSV) | `gui/dxf_viewer/messdaten_eingabe.rs` `schreibe_anschlusspunkt` | AP-Messungen der aktiven Aufstellung | Ergebnisdatei, wird bei Neuer Stp archiviert |

Dazu `Station-*.json` (Zustand der Aufstellung) und das Kommunikationslog (Rohverkehr mit dem Gerät).

## 2. Gefundene Lücken

Geordnet nach Gewicht. „TP“ = Tagesprotokoll `Data-protocol-*.csv`.

### Kritisch – Daten gehen dem Protokoll verloren

- **L1 – Anschlusspunktmessungen fehlen im TP.** Messungen im Dialog „Messdaten eingeben“ laufen über
  `messergebnis_fuer_dialog` am TP vorbei und landen nur in der Anschlusspunktdatei. Die Stationierung ist damit
  aus dem TP **nicht** rekonstruierbar. (Nur die Kontrollmessung `KP` und Neupunkte stehen im TP.)
- **L2 – Handeingaben und Korrekturen im Dialog fehlen.** Hz, V, Strecke, Zielhöhe und Code können im Dialog
  nach der Messung überschrieben oder ganz von Hand eingegeben werden. Weder der Originalwert noch die Änderung
  wird protokolliert, die Zeile in der stp-Datei unterscheidet nicht zwischen „gemessen“ und „von Hand“.
- **L3 – Keine AufstellungId im TP.** Messungen sind nur über StationNr + IH + Zeit einer Aufstellung zuordenbar.
  Bei gleicher Nummer (Fortsetzen, Zusammenführen) oder Mitternacht (neue Tagesdatei) ist das mehrdeutig.
- **L4 – Kein Typ (AP/NP/KP) und keine Exzentrizitäten im TP.** Die Spalte „Method“ enthält Freitext
  („Neupunkt“, „Stationierung“, „Kontrolle“); Exzentrizitäten stehen nur in der dat.csv.
- **L5 – Koordinaten der Anschlusspunkte fehlen im TP.** Für die stp.csv braucht man R, H, Höhe des identischen
  Punkts (aus Snap/Feldbuchpunkten). Diese werden nirgends protokolliert.
- **L6 – Korrektur der Instrumentenhöhe steht nicht im TP.** `set_instrumenten_hoehe` schreibt nur ins
  abschaltbare Textprotokoll; die betroffenen Neupunkte werden still neu gerechnet.

### Wichtig – Änderungen sind nicht nachvollziehbar

- **L7 – Punktnummer überschrieben.** Misst man einen Neupunkt mit vorhandener Nummer in derselben Aufstellung,
  ersetzt `hinzufuegen_oder_ersetzen` den alten Datensatz ohne Vermerk. Manuelle Änderung der vorgeschlagenen
  Nummer und Sprünge des Neupunktzählers werden nicht vermerkt.
- **L8 – Prisma ↔ Reflektorlos und Prismentyp.** Die Umschaltung steht nur im Kommunikationslog. EDM-Modus und
  Prismenkonstante stehen zwar in jeder Messzeile, ein Wechsel des Prismentyps in den Einstellungen
  (z. B. Rundprisma → 360°) aber nicht.
- **L9 – Zielhöhe.** Steht pro Messung im TP (gut). Wird sie im Dialog nach der Messung korrigiert, ist das nicht
  sichtbar (siehe L2).
- **L10 – Getrennte Messung.** Strecke aus der Messung, Hz/V von den Live-Werten – im TP nicht als solche
  gekennzeichnet.
- **L11 – Stationierungsergebnis nicht im TP.** Ergebnis (R, H, Höhe, Orientierung, Maßstab, s0, verwendete
  Punkte, deaktivierte Richtungen/Strecken) steht nur als Kommentar in der dat.csv und im Textprotokoll.
- **L12 – Löschen und Nachbearbeiten.** Löschen von Punkten („n Punkte gelöscht“ ohne Nummern),
  Exzentrizitätsänderungen in der Polarpunktberechnung und Änderungen im Datenmanager werden nicht oder nur
  summarisch vermerkt.
- **L13 – Messungen im „Stationierungsmodus“ außerhalb des Dialogs** werden ins TP geschrieben, gehören aber zu
  keinem Punkt (Status: „nur im Tagesprotokoll gespeichert“). Sie wären beim Wiederherstellen Datenmüll.

### Formal

- **L14 – Keine laufende Nummer.** Zeilen sind nicht eindeutig referenzierbar (für Korrekturen „bezieht sich auf
  Zeile …“ nötig).
- **L15 – Kommentarzeilen sind Freitext** (`# NEUER STANDPUNKT: SP=… IH=…`) und müssten zum Wiederherstellen
  geparst werden.
- **L16 – Rohdaten des Geräts** (`TachymeterMessung::rohdaten`, GSI/GeoCOM-Antwort) werden nicht gespeichert.
- **L17 – Tagesdatei.** Aufteilung nach Tagen ist fürs Feld praktisch, für das Wiederherstellen eines Projekts
  muss man alle Tagesdateien in richtiger Reihenfolge einlesen.

## 3. Vorschlag: Ereignisprotokoll pro Projekt

### 3.1 Grundsatz

Ein neues, **nicht abschaltbares**, nur anhängendes Protokoll `{Projekt}-Messprotokoll.csv` im Projektordner
mit **festen Spalten** und **einer Zeile pro Ereignis**. Nichts wird überschrieben: Korrekturen, Umbenennungen und
Löschungen sind eigene Zeilen, die über `Bezug` auf die laufende Nummer der ursprünglichen Zeile verweisen.

Das neue Protokoll **ersetzt** `Data-protocol-*.csv` (P2). Es übernimmt deshalb alle Spalten des bisherigen
Tagesprotokolls (auch Kompensator und GNSS). Vorhandene `Data-protocol-*.csv` bleiben als Altdateien liegen und
werden beim Import von Altprojekten (P7) mit eingelesen. Das Textprotokoll `Protokoll_*.txt` bleibt Diagnose.
Im Programm liest niemand `Data-protocol-*.csv` ein; das Ersetzen bricht also keine Funktion.

### 3.2 Spalten

| Spalte | Inhalt |
|---|---|
| `LfdNr` | fortlaufend im Projekt, eindeutig |
| `Zeit` | ISO 8601 mit Offset (wie `messzeit`) |
| `Ereignis` | siehe 3.3 |
| `AufstellungId`, `StandpunktNr`, `IH_m` | aktive Aufstellung zum Zeitpunkt |
| `PunktNr`, `Typ`, `Code` | Typ = AP / NP / KP |
| `Hz_gon`, `V_gon`, `S_m`, `ZH_m`, `ExzL_m`, `ExzQ_m` | Messwerte |
| `R`, `H`, `Hoehe` | Sollkoordinaten (AP/KP) oder berechnete Koordinaten (NP, Stationierung) |
| `EDM`, `Prismentyp`, `PrismKonst_mm`, `ppm`, `Druck_mbar`, `Temp_C`, `NeigL_mgon`, `NeigQ_mgon`, `Kompensator_OK` | Gerätezustand (wie bisheriges Tagesprotokoll) |
| `GNSS_Breite`, `GNSS_Laenge`, `GNSS_Hoehe_m`, `GNSS_GenLage_m`, `GNSS_GenHoehe_m`, `GNSS_Fix`, `GNSS_Sat`, `GNSS_HDOP`, `GNSS_PDOP` | aus dem bisherigen Tagesprotokoll übernommen (heute leer) |
| `Quelle` | `Geraet` / `Hand` / `Getrennt` / `Korrigiert` / `Import` |
| `Bezug` | LfdNr der Zeile, auf die sich Korrektur/Löschung/Umbenennung bezieht |
| `Feld`, `Alt`, `Neu` | bei Änderungen: was, von, nach |
| `Text` | Freitext (Bemerkung, Meldung) |
| `Roh` | Rohantwort des Geräts (L16, P3: wird immer mitgeschrieben; `;` und Zeilenumbrüche maskiert) |

### 3.3 Ereignisse

| Ereignis | Auslöser heute | schließt Lücke |
|---|---|---|
| `PROJEKT` | Projekt geöffnet/angelegt | – |
| `GERAET` | Verbinden/Trennen, Modell, Seriennummer | – |
| `EDM` | Prisma ↔ Reflektorlos (kurz/lang) | L8 |
| `PRISMA` | Prismentyp/-konstante geändert (Einstellungen) | L8 |
| `ATMOS` | ppm/Druck/Temperatur geändert | – |
| `STP_NEU`, `STP_FORT` | Dialog „Neuer Stp“ | L3, L15 |
| `STP_UMBENANNT` | Umbenennen/Zusammenführen | L15 |
| `IH` | Korrektur der Instrumentenhöhe (Alt/Neu) | L6 |
| `MESSUNG` | jede übernommene Messung (AP, NP, KP) mit allen Werten | L1, L4, L5, L10 |
| `KORREKTUR` | Feldwert einer Messung nach der Messung geändert (Bezug, Feld, Alt, Neu) | L2, L9 |
| `NUMMER` | Punktnummer einer Messung geändert (Bezug, Alt, Neu) | L7 |
| `ERSETZT` | gleiche Nummer erneut gemessen – alte Messung wird ersetzt (Bezug) | L7 |
| `GELOESCHT` | Messung/Punkt gelöscht oder deaktiviert (Bezug) | L12 |
| `EXZ` | Exzentrizität nachträglich geändert (Bezug, Alt, Neu) | L12 |
| `STATIONIERUNG` | übernommenes Ergebnis: R, H, Höhe, Ori, Maßstab, s0, n; inaktive Richtungen/Strecken im Text | L11 |
| `VERWORFEN` | Messung ohne Punktbezug (heutiger L13-Fall) – wird beim Wiederherstellen übersprungen | L13 |
| `BEMERKUNG` | freie Notiz des Messtrupps (optional, Button) | – |

### 3.4 Wiederherstellen (spätere Funktion „Aus Protokoll ableiten“)

Das Protokoll wird von vorne nach hinten „abgespielt“:

1. `MESSUNG`-Zeilen bilden die Grunddaten; `KORREKTUR`, `NUMMER`, `EXZ` ändern sie über `Bezug`;
   `ERSETZT`/`GELOESCHT` nehmen sie heraus.
2. Gruppierung nach `AufstellungId`:
   - **stp.csv** je Aufstellung: Kopf aus `STP_NEU`/`STP_FORT`/`IH`, Zeilen aus AP-Messungen mit R/H/Höhe.
   - **dat.csv**: alle NP- und KP-Messungen mit AufstellungId und IH (Format wie heute, `SPALTEN_DAT`).
   - **Koordinaten**: aus `STATIONIERUNG` + NP-Messungen neu gerechnet (Polarpunktberechnung), alternativ die
     protokollierten Koordinaten.
3. Ausgabe in einen neuen Ordner (`Wiederhergestellt-JJJJMMTT-HHMMSS/`), nie über die vorhandenen Dateien.
   Ein Vergleich mit den aktuellen Projektdateien zeigt Abweichungen an.
4. Auswahl: ganzes Projekt, einzelne Aufstellungen oder Zeitraum.

### 3.5 Technische Umsetzung (Skizze)

- Neues Modul `daten/messprotokoll.rs` mit `enum Ereignis` und **einer** Schreibfunktion
  `protokolliere(&Einstellungen, Ereignis) -> io::Result<LfdNr>`. Die laufende Nummer liest das Modul beim
  Öffnen des Projekts aus der letzten Zeile.
- Die vorhandenen Schreibstellen rufen zusätzlich `protokolliere` auf: `verarbeite_messung`,
  `schreibe_anschlusspunkt`, `schreibe_kontrollmessung`, `standpunkt_beginnen/_fortsetzen`,
  `set_instrumenten_hoehe`, `standpunkt_umbenennen`, `uebernehme_*_stationierung`, Löschen im Viewer,
  `schreibe_exzentrizitaeten`, Geräteschalter, Einstellungsdialog.
- Der Messdaten-Dialog merkt sich die Gerätewerte (`mess_hz`, … gibt es teils schon) und schreibt beim
  Übernehmen `MESSUNG` mit Quelle `Geraet` + je geändertem Feld eine `KORREKTUR`-Zeile; bei reiner Handeingabe
  `MESSUNG` mit Quelle `Hand`.
- Schreibfehler des Messprotokolls blockieren die Messung **nicht**, werden aber deutlich angezeigt (rote Meldung),
  damit eine Lücke nicht unbemerkt bleibt.

## 4. Entscheidungen (22.11.2026)

- **P1 – eine Datei pro Projekt:** `{Projekt}-Messprotokoll.csv`.
- **P2 – ersetzen:** `Data-protocol-*.csv` wird nicht mehr geschrieben; das neue Protokoll übernimmt dessen
  Spalten (siehe 3.1, 3.2).
- **P3 – Rohdaten mitschreiben:** Spalte `Roh` immer gefüllt, soweit das Gerät eine Antwort liefert.
- **P4 – kein eigenes Ereignis `ZH`:** Die Zielhöhe steht in jeder Messzeile; Änderungen nach der Messung im
  Dialog sind `KORREKTUR`-Zeilen.
- **P5 – keine Prüfsumme.**
- **P6 – Koordinaten neu rechnen** aus `STATIONIERUNG` und den Messungen.
- **P7 – Altprojekte importieren:** Beim ersten Öffnen eines Projekts ohne Messprotokoll wird einmalig ein
  Start-Protokoll aus vorhandener `dat.csv`, Anschlusspunktdateien (`Station-*`, stp-CSV) und
  `Data-protocol-*.csv` erzeugt (Quelle `Import`). Die Quelldateien bleiben unverändert.

## 5. Arbeitsplan

- [x] 1. `daten/messprotokoll.rs`: Ereignis-Typen, Spalten, Schreiben, LfdNr, Maskierung `Roh`, Tests
- [x] 2. Messungen: Neupunkt, Anschlusspunkt (Dialog), Kontrolle, getrennte Messung (L1, L4, L5, L10, L16)
- [x] 3. Dialog „Messdaten eingeben“: Gerätewerte merken, Korrekturen/Handeingabe protokollieren (L2, L9)
- [x] 4. Standpunkt: Neu, Fortsetzen, Umbenennen, IH-Korrektur, Stationierungsergebnis (L3, L6, L11)
- [x] 5. Gerät/Einstellungen: EDM, Prismentyp, Atmosphäre (L8)
- [x] 6. Nummern, Ersetzen, Löschen, Exzentrizitäten (L7, L12), Stationierungsmodus-Messung als `VERWORFEN` (L13)
- [x] 7. `schreibe_tagesprotokoll` und `Data-protocol-*.csv` stilllegen (P2); Kommentarzeilen in der dat.csv bleiben
- [x] 8. Import Altprojekte (P7)
- [ ] 9. Funktion „Aus Protokoll ableiten“ (stp.csv, dat.csv, Koordinaten neu gerechnet) mit Vergleich

## 6. Umsetzungsstand (22.11.2026)

Modul [src/daten/messprotokoll.rs](src/daten/messprotokoll.rs): Datei `{Projekt}-Messprotokoll.csv`, 43 Spalten,
laufende Nummer (`LfdNr`), nur Anhängen. Schreibfehler blockieren nichts, erscheinen aber im Hinweisfenster.

| Ereignis | geschrieben in |
|---|---|
| `PROJEKT` | `Projekt::messprotokoll_oeffnen` (Programmstart, Projektwechsel) |
| `GERAET`, `EDM`, `PRISMA` | `messprotokoll::Beobachter` im Hauptfenster: erkennt Verbinden/Trennen, Prisma ↔ Reflektorlos und Prismenwechsel unabhängig davon, wo sie ausgelöst wurden |
| `STP_NEU`, `STP_FORT` | `messdaten::schreibe_standpunktwechsel` (mit Code, bekanntem Punkt, Gerätezustand) |
| `STP_UMBENANNT` | `Projekt::standpunkt_umbenennen` |
| `IH` | `Projekt::set_instrumenten_hoehe` (alt → neu) |
| `MESSUNG` NP | DXF-Viewer `verarbeite_neupunkt`; Quelle `Getrennt` bei getrennter Messung; Hinweis bei Punktnummer abweichend vom Vorschlag |
| `ERSETZT` | gleiche Nummer in derselben Aufstellung erneut gemessen (Bezug auf die alte Messung) |
| `MESSUNG` AP + `KORREKTUR` | Dialog „Messdaten eingeben“: Gerätewerte als `MESSUNG`, jede spätere Abweichung von Hz/V/S/ZH als `KORREKTUR` (Tastatur = `Hand`, nachgeführte Winkel = `Getrennt`); reine Handeingabe = `MESSUNG` mit Quelle `Hand`; Hinweis bei geänderter Nummer/Höhe gegenüber dem Plan |
| `MESSUNG` KP | Orientierungskontrolle (mit Abweichungen im Text) |
| `VERWORFEN` | Messung im Stationierungsmodus ohne Punktbezug |
| `STATIONIERUNG` | `Projekt::uebernehme_manuelle_stationierung` (R/H/Höhe, Ori, Maßstab, s0, n, inaktive Beobachtungen, Anschlusspunkte) |
| `GELOESCHT` | Löschen im DXF-Viewer, je Punkt (Neupunkte mit Bezug auf ihre Messung) |
| `EXZ` | Exzentrizität in der Polarpunktberechnung geändert (Bezug auf die Messung) |
| `BEMERKUNG` | Datei im Datenmanager gespeichert; Abschluss des Start-Protokolls |
| `ALTMESSUNG` | Import alter `Data-protocol-*.csv` (nur Nachweis) |

- **Rohdaten (P3):** Bei GeoCOM enthält `Roh` jetzt die Ergebnisantwort und alle Zusatzantworten
  (Kompensator, Atmosphäre, EDM, Prisma), getrennt durch ` | `; bei GSI die Messzeile.
- **Data-protocol (P2):** `schreibe_tagesprotokoll` und `Data-protocol-*.csv` sind entfernt.
- **Import (P7):** `messprotokoll::erzeuge_startprotokoll` beim Öffnen eines Projekts ohne Messprotokoll:
  Stationsdateien (aktive und Archive, je AufstellungId die vollständigste) → `STP_NEU`, AP-`MESSUNG`,
  `STATIONIERUNG`; Messdatei → NP/KP-`MESSUNG`; `Data-protocol-*.csv` → `ALTMESSUNG`; zeitlich sortiert.
- **Noch nicht belegt:** `NUMMER` (Umbenennen einer bereits gespeicherten Messung) – das Programm hat dafür
  derzeit keine eigene Funktion; Änderungen im Datenmanager stehen nur als `BEMERKUNG` ohne Feldvergleich.
- **Offen:** Schritt 9 „Aus Protokoll ableiten“.
