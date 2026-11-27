# GNSS-Vermessung in Feldbuch-NT – Vorschlag

Stand: 26.11.2026 · Status: **umgesetzt in Version 0.10.1 (Abschnitt 15), mit Empfänger noch nicht geprüft**

Ziel: Das Programm `gnss-vermessung` (RTK mit simpleRTK2B / u-blox ZED-F9P und SAPOS HEPS) wird als eigenes
Modul in Feldbuch-NT eingebunden. Konfiguration und Funktion des GNSS-Programms bleiben unverändert. Feldbuch-NT
startet und überwacht damit die GNSS-Messung, zeigt die Antennenposition live im DXF-Viewer, misst Punkte, steckt
ab und passt GNSS-Koordinaten über identische Punkte (T-Punkte) in das Projektsystem ein.

Inhalt

1. Ausgangslage
2. Einbindung des GNSS-Programms (Architektur)
3. Hauptmenü: Modul „GNSS-Vermessung“
4. Messgerät-Kommunikation: Umschaltung Tachymeter ↔ GNSS
5. DXF-Viewer im GNSS-Betrieb
6. Messen mit GNSS
7. GNSS-Absteckung
8. Einpassung (Transformation über T-Punkte)
9. Rechenparameter: neue Gruppe „GNSS“
10. Messprotokoll und Berechnungsprotokolle
11. Dateien und Datenmodell
12. Eigene Vorschläge und Ideen
13. Umsetzungsschritte
14. Entscheidungen
15. Umsetzungsstand

---

## 1. Ausgangslage

### 1.1 gnss-vermessung (Version 0.4.0)

| Teil | Inhalt | Bedeutung für die Einbindung |
|---|---|---|
| Bibliothek `gnss_vermessung` (`src/lib.rs`) | `engine` (Threads: seriell lesen/schreiben, NTRIP, Konfiguration, Aufzeichnung, Wiedergabe), `model::State` (Lösung, Satelliten, DOP, NTRIP-Zustand, Spur), `measure` (Kriterien, Epochensammlung, Mittelung), `geo` (ETRS89 ↔ UTM32, Maßstab, Meridiankonvergenz), `height` / `transform` (DHHN2016 über Quasigeoid oder SAPOS 1021/1023), `settings` (TOML + Passwort im Windows-Tresor), `export` | wird unverändert in den Feldbuch-Zweig kopiert (`src/gnss/kern/`) |
| Oberfläche `src/bin/gnss-gui/*` | `App` mit 13 Reitern (Status, Großanzeige, Messen, Satelliten, DOP, Zeit, Karte, Spur, Korrekturdaten, Rohdaten, Konfiguration, Werkzeuge, Hilfe) | wird in den Feldbuch-Zweig kopiert und dort zur Ansicht umgebaut (Abschnitt 2.2, 2.4); im Original bleibt sie, wie sie ist |
| Einstellungen | `%APPDATA%\gnss-vermessung\settings.toml`, Passwort im Windows-Tresor (`keyring`) | Original unverändert; Feldbuch bekommt eigene Einstellungen mit einmaliger Übernahme (2.5) |

Beide Programme verwenden `eframe 0.37` und Edition 2024 – die Oberfläche lässt sich daher ohne Umbau der
Zeichenlogik in Feldbuch-NT einbetten.

### 1.2 Feldbuch-NT (Version 0.10.0)

- `TachymeterModell::GnssNmea` und `tachymeter/gnss.rs` (NMEA-Datenmodell) existieren, werden aber nirgends
  ausgewertet.
- Das Messprotokoll hat bereits 9 GNSS-Spalten (`GNSS_Breite` … `GNSS_PDOP`), die bisher leer bleiben.
- `rechnen/koord_transformation.rs` bietet 2D-Helmert (mit/ohne festen Maßstab), 3D-Helmert, 7-Parameter
  Bursa-Wolf, 9-Parameter und 6-Parameter (fester Maßstab) mit Ausgleichung und Residuen; die Oberfläche
  `gui/koord_transformation.rs` zeigt Residuen mit Ampel und Aktiv-Schalter je Punkt.
- Die Punktabsteckung im DXF-Viewer (`gui/dxf_viewer/absteckung.rs`) bringt Zoomstufen, Ampel,
  Toleranzen aus den Rechenparametern, „Abgesteckt“-Logik und Absteckprotokoll mit.

---

## 2. Einbindung des GNSS-Programms (Architektur)

### 2.1 Grundsatz: zwei getrennt gepflegte Programme (Entscheidung 14.6)

- **gnss-vermessung bleibt unverändert** – eigenes Repository, eigenes `gnss-gui.exe` und
  `gnss-vermessung.exe`, eigene Releases, eigene Update-Funktion. Es wird weiter gepflegt. Für die Einbindung
  wird dort **nichts** geändert.
- **Feldbuch-NT bekommt eine eigene Kopie** des Codes zum Stand **gnss-vermessung 0.4.0 (Commit `7526a36`)**.
  Ab der Übernahme wird dieser Teil **unabhängig** im Feldbuch-Repository gepflegt. Es gibt keine
  Cargo-Abhängigkeit auf `../GNSS-NT`; Feldbuch-NT baut auch, wenn der Ordner fehlt.
- Verbesserungen werden zwischen den beiden Programmen **nur bewusst und von Hand** übertragen (z. B. eine
  Fehlerkorrektur im NTRIP-Client). Dafür gibt es einen Herkunftsvermerk (2.3).

Verworfene Alternativen:

| Variante | Warum nicht |
|---|---|
| Pfadabhängigkeit auf `../GNSS-NT` | koppelt die Pflege: jede Änderung im Original wirkt sofort im Feldbuch und umgekehrt; Umbau des Originals nötig |
| `gnss-gui.exe` als eigener Prozess | zwei Fenster auf dem Toughpad, COM-Port nicht teilbar, eigene Schnittstelle für Live-Positionen nötig |

### 2.2 Ablage im Feldbuch

```
src/gnss/
  HERKUNFT.md          – Herkunftsvermerk je Datei (2.3)
  mod.rs               – Anbindung an Feldbuch: Sitzung, LivePosition, Messquelle
  kern/                – übernommen aus gnss-vermessung/src (Bibliothek)
    engine.rs  model.rs  measure.rs  survey.rs  geo.rs  height.rs  transform.rs
    ntrip.rs  rtcm.rs  rtcm_decode.rs  ubx.rs  ubxmsg.rs  nmea.rs  demux.rs
    dop.rs  spp.rs  gtime.rs  export.rs  settings.rs  config.rs
  ansicht/             – übernommen aus gnss-vermessung/src/bin/gnss-gui (Reiter)
    app.rs  view_status.rs  view_measure.rs  view_sky.rs  view_dop_anim.rs  view_time.rs
    view_map.rs  map_stations.rs  tiles.rs  view_track.rs  view_ntrip.rs  view_raw.rs
    view_ephem.rs  view_config.rs  widgets.rs
```

**Nicht übernommen:** Kommandozeilenprogramm (`main.rs`, `menu.rs`, `session.rs`), Update-Funktion
(`update.rs`, `view_update.rs`, `version.rs`), Bildschirmfoto-Modus, `tools/Release-Paket.ps1`, die Abhängigkeiten
`clap` und `rpassword`.

Namen im übernommenen Code (englisch, Kommentare deutsch) bleiben zunächst so, wie sie sind. Das hält den Code
mit dem Original vergleichbar, solange noch Korrekturen übertragen werden. Kopfkommentare bekommen den Zusatz
„Übernommen aus gnss-vermessung 0.4.0, eigenständig gepflegt in Feldbuch-NT“.

### 2.3 Herkunftsvermerk

`src/gnss/HERKUNFT.md` listet je Datei: Ursprungsdatei, Ursprungs-Commit, Datum der Übernahme und danach jede
bewusst übertragene Änderung („aus gnss-vermessung 0.4.2 übernommen: NTRIP-Reconnect“) – in beide Richtungen.
So bleibt nachvollziehbar, welcher Stand im Feldbuch steckt, ohne dass die Programme technisch gekoppelt sind.

### 2.4 Anpassungen im Feldbuch-Zweig

1. **Oberfläche als Ansicht:** `App::new` braucht heute einen `CreationContext`. Neu
   `App::eingebettet(ctx: &egui::Context)` und `App::zeichne(&mut self, ui: &mut egui::Ui)` (Inhalt von
   `eframe::App::ui`), damit Feldbuch-NT die Reiter in einem `CentralPanel` zeichnet.
2. **Schnittstelle für Feldbuch-NT** (in `src/gnss/mod.rs`):
   - `fn position(&self) -> Option<LivePosition>` – letzte Lösung (lat, lon, h, Fix, σ E/N/H, Kovarianz,
     PDOP, Satelliten, Korrekturalter, Referenzstation, GPS-Zeit), dazu UTM32 und Höhe nach eingestelltem
     Höhenmodell.
   - `fn starte_messung(&mut self, anfrage: PointRequest, kriterien: Criteria)`,
     `fn messung_status(&self) -> Option<MessFortschritt>`, `fn nimm_ergebnis(&mut self) -> Option<PointResult>`,
     `fn breche_messung_ab(&mut self)`.
   - `fn verbunden(&self)`, `fn verbinde(&mut self)`, `fn trenne(&mut self)`, `fn ntrip_status(&self)`.
3. **Keine eigene Update-Funktion:** Das GNSS-Modul wird mit Feldbuch-NT ausgeliefert und über dessen Update
   aktualisiert. Die Einstellung `check_updates` und der Menüpunkt Hilfe → Update entfallen im Feldbuch-Zweig.
4. **Erscheinungsbild:** Die Reiter übernehmen Schriftgröße und Hell/Dunkel von Feldbuch-NT; die Einstellung
   „Darstellung“ (dark, scale) setzt nicht mehr den globalen Stil.
5. **Tastenkürzel** (Leertaste = Messen, F5 = Verbinden) nur, solange die GNSS-Ansicht sichtbar ist; sonst
   kollidieren sie mit dem DXF-Viewer.

Funktion und Konfigurationsmöglichkeiten der Reiter bleiben beim Übernehmen **gleich**; geändert wird nur, was
für die Einbettung nötig ist.

### 2.5 Einstellungen und Zugangsdaten

- **Eigene Einstellungsdatei** in den Feldbuch-Benutzerdaten: `%LOCALAPPDATA%\Feldbuch\gnss.toml`
  (`pfade::benutzer_get`). Aufbau und Inhalt wie `settings.toml` des Originals.
- **Einmalige Übernahme:** Fehlt `gnss.toml` und gibt es `%APPDATA%\gnss-vermessung\settings.toml`, bietet
  Feldbuch-NT beim ersten Öffnen an, die Einstellungen (COM-Port, SAPOS-Benutzer, Mountpoint, Messvorgaben,
  Antenne, Höhenmodell) zu übernehmen. Danach sind beide getrennt.
- **SAPOS-Passwort:** eigener Eintrag im Windows-Tresor (Dienst `feldbuch-nt-sapos`), bei der Übernahme auf
  Wunsch aus dem Eintrag `gnss-vermessung-sapos` kopiert. Das Passwort steht nie in einer Datei.
- **Kartenkacheln** dürfen gemeinsam genutzt werden (`%LOCALAPPDATA%\gnss-vermessung\tiles`, nur Lesen und
  Ergänzen), damit offline gespeicherte Karten in beiden Programmen verfügbar sind. Fehlt der Ordner, legt
  Feldbuch einen eigenen an.

### 2.6 Lebensdauer der GNSS-Sitzung

- Die Sitzung gehört **nicht** zur Ansicht, sondern zu `FeldbuchApp` (wie `geraet`):
  `app.gnss: Option<Box<gnss::Sitzung>>`.
- Sie läuft weiter, wenn man ins Hauptmenü oder in den DXF-Viewer wechselt (Empfänger, NTRIP, Fix bleiben
  erhalten). Erst „Trennen“ oder Programmende beendet sie.
- `Engine` benachrichtigt über `Notify` → `ctx.request_repaint()`; der DXF-Viewer wird also bei jeder neuen
  Epoche (1 Hz, einstellbar bis 10 Hz) neu gezeichnet, ohne Dauerschleife.
- **Empfängerkonfiguration dauerhaft** (Entscheidung 14.7): Beim Verbinden aus Feldbuch-NT wird die
  Konfiguration in RAM, BBR und Flash geschrieben (`ubx::LAYER_RAM | LAYER_BBR | LAYER_FLASH`, im Original
  bereits als `Action::Configure { persist: true }` vorhanden). Um den Flash zu schonen, wird nur geschrieben,
  wenn sich die Werte geändert haben: erst Rücklesen (`readback`), mit den Sollwerten vergleichen, nur
  Abweichungen speichern. Nach dem Trennen und beim nächsten Einschalten arbeitet der Empfänger mit derselben
  Konfiguration weiter; der Zeitpunkt der letzten dauerhaften Speicherung steht im Messprotokoll
  (`GNSS_VERBINDUNG`).
- **Beide Programme am selben Empfänger:** Der COM-Port kann nur von einem Programm geöffnet sein. Ist er belegt,
  meldet Feldbuch-NT „COM5 belegt – läuft gnss-gui.exe?“. Da beide Programme die Konfiguration dauerhaft
  schreiben können, würden unterschiedliche Einstellungen (z. B. Messrate) sich gegenseitig im Flash
  überschreiben; der Rücklesevergleich zeigt beim Verbinden an, welche Werte abweichen, bevor geschrieben wird.

### 2.7 Bau und Abhängigkeiten

- Neue Abhängigkeiten in `Feldbuch-NT/Cargo.toml`: `anyhow`, `keyring` (windows-native), `toml`, `base64`.
  Bereits vorhanden: `chrono`, `serialport`, `serde`, `serde_json`, `ureq`, `image`, `rfd`, `eframe 0.37`.
- Die Tests des Originals (UTM gegen PROJ < 1 mm, Maßstab/Konvergenz, Messkriterien, RTCM-Dekodierung) werden
  mit übernommen und laufen mit `cargo test` von Feldbuch-NT.
- `demo.ubx` kommt nach `testdaten/gnss/` für automatische Tests und den Simulationsmodus (12.7).
- Die Info-Seite von Feldbuch-NT nennt „GNSS-Modul: abgeleitet von gnss-vermessung 0.4.0“.

---

## 3. Hauptmenü: Modul „GNSS-Vermessung“

- Die erste Zeile im Hauptmenü wird geteilt: **DXF-Viewer** (breit, wie bisher) und daneben
  **GNSS-Vermessung** (gleiche Höhe, eigene Farbe, eigenes Icon: Antenne mit Satellitenbogen).
- Der GNSS-Knopf zeigt eine kleine **Statuslampe**: grau = nicht verbunden, rot = kein Fix/autonom,
  gelb = RTK Float, grün = RTK Fix. So sieht man schon im Hauptmenü, ob die Messung läuft.
- Klick öffnet `Ansicht::Gnss`: die komplette GNSS-Oberfläche mit allen Reitern, oben die Feldbuch-Leiste
  „← Zurück zum Hauptmenü“ und rechts davon zusätzlich **„→ DXF-Viewer“**, damit man ohne Umweg zwischen
  Überwachung und Plan wechselt.
- Ist im Projekt der Gerätetyp nicht GNSS, kann das Modul trotzdem geöffnet werden (Überwachen, Konfigurieren,
  eigenständige Punktmessung wie bisher). Die Übernahme in den DXF-Viewer setzt die Umschaltung nach
  Abschnitt 4 voraus.

**Reiter „Messen“ (Entscheidung 14.5):** Der Reiter bleibt im eingebetteten Modus eigenständig nutzbar und
schreibt weiterhin seine eigenen Dateien (`<Name>-kor.csv`, `.kor`, `-protokoll.csv`). Im eingebetteten Modus
wird als Vorgabe für die Punktdatei der Projektordner von Feldbuch-NT vorgeschlagen
(`<Projekt>\GNSS\<Projekt>-gnss-kor.csv`), damit nichts im Dokumente-Ordner verschwindet. Die eigentliche
Projektmessung (mit Einpassung, Plan und Messprotokoll) läuft über den DXF-Viewer (Abschnitt 6); Messungen aus
dem Reiter erscheinen nicht automatisch im Projekt.

---

## 4. Messgerät-Kommunikation: Umschaltung Tachymeter ↔ GNSS

### 4.1 Umbenennung

- Knopf im Hauptmenü: „Tachymeter-Kommunikation“ → **„Messgerät-Kommunikation“** (`hauptfenster.rs`,
  `MODUL_ZEILEN`, `oeffne_modul`, Hilfetext der Info-Seite).
- Überschrift der Ansicht ist bereits „Messgerät – Kommunikation“.

### 4.2 Neuer Gerätetyp

- Neuer Eintrag in `TachymeterModell`: **`GnssSapos`**, Anzeige **„GNSS – SAPOS (u-blox ZED-F9P)“**.
- Der bisherige Eintrag `GnssNmea` („GNSS (NMEA 0183)“) **entfällt** (Entscheidung 14.1), ebenso das
  ungenutzte NMEA-Datenmodell `tachymeter/gnss.rs`. Alte Projekteinstellungen mit `GnssNmea` bzw.
  `IstGnssGeraet=true` werden beim Laden auf `GnssSapos` umgestellt (einmaliger Hinweis).
- Gespeichert wird wie bisher in den Projekteinstellungen (`tachymeter_modell`); `ist_gnss_geraet()` gilt dann
  für `GnssSapos`.

### 4.3 Verhalten bei „GNSS – SAPOS“

- Eine bestehende Tachymeterverbindung wird nach Rückfrage getrennt.
- Ausgeblendet: Sprache am Gerät, Schnittstellenparameter, Koppeln, Analyse, Befehlsprotokoll.
- Stattdessen eine Gruppe **„GNSS-Empfänger“**:
  - Status: verbunden/getrennt, COM-Port, Fix, Satelliten, σ Lage/Höhe, Korrekturalter, Mountpoint,
    Höhenbezug (ellipsoidisch / GCG2016 / SAPOS 1021–1023 / konstante Höhenanomalie).
  - Knöpfe **„Verbinden“ / „Trennen“** (rufen die GNSS-Sitzung auf, inkl. Empfängerkonfiguration und SAPOS-Start
    wie in gnss-gui) und **„GNSS-Konfiguration öffnen“** (springt in `Ansicht::Gnss`, Reiter Konfiguration).
  - Hinweis, dass COM-Port, SAPOS-Zugang, Messvorgaben und Antenne im GNSS-Modul eingestellt werden.
- Zurück auf ein Tachymeter: GNSS-Sitzung wird nach Rückfrage getrennt.

### 4.4 Messquelle

Neu in `FeldbuchApp` bzw. im DXF-Viewer: `enum Messquelle { Tachymeter, Gnss }`, abgeleitet aus dem Gerätetyp.
Alle Stellen, die heute `geraet` zum Messen benutzen, fragen zuerst die Messquelle ab. Damit bleibt der
Tachymeter-Code unangetastet; der GNSS-Zweig kommt daneben.

---

## 5. DXF-Viewer im GNSS-Betrieb

### 5.1 Live-Position

- **Antennensymbol** an der aktuellen Position (Kreuz mit Kreis), Farbe nach Lösung: grün RTK Fix,
  gelb Float, orange DGPS, rot autonom, grau veraltet (> 3 s keine Epoche).
- **Fehlerellipse** aus der Kovarianz (NAV-COV, `Solution::ellipse`) im Planmaßstab, zuschaltbar;
  bei Fix meist kleiner als das Symbol, bei Float deutlich sichtbar.
- **Spur** der letzten Positionen (Länge einstellbar, z. B. 200 Epochen), farbig nach Lösungsart – wie der
  Reiter „Spur“, aber im Plan.
- **Bewegungspfeil**: Richtung und Geschwindigkeit aus der Spur (siehe 7.3).
- **„Folgen“**: Schalter, der den Plan mit der Antenne mitführt (Antenne bleibt in der Bildmitte, Zoom bleibt).
  Beim Verschieben von Hand schaltet „Folgen“ aus.
- Liegt die Antenne außerhalb des sichtbaren Ausschnitts: Randpfeil mit Entfernung (wie heute die
  Zielrichtung).

### 5.2 Statuszeile (statt Standpunktinfo)

```
RTK FIX 00:12:41 · 23 Sat · PDOP 1.3 · σL 8 mm · σH 14 mm · Alter 1.0 s · VRS_3_4G_BY · H: DHHN2016 (SAPOS)
Einpassung: E-2026-11-26-1 (2D-Helmert, 5 T-Punkte, s0 6 mm)
```

- Zeile 2 nur bei aktiver Einpassung; ohne Einpassung: „Koordinaten: ETRS89/UTM32 (ohne Einpassung)“.
- Bei Fix-Verlust färbt sich die Zeile rot, und es ertönt ein Warnton (`grundlagen/ton.rs`).

### 5.3 Toolbar im GNSS-Betrieb

| Knopf | GNSS-Betrieb |
|---|---|
| Messen | **sichtbar** – GNSS-Messung (Abschnitt 6) |
| Getrennt | ausgeblendet |
| Neuer Stp / Kontrolle Standpunkt | ersetzt durch **Einpassung** und **Kontrollpunkt** (Abschnitt 8) |
| Abstecken | **sichtbar** – GNSS-Absteckung (Abschnitt 7) |
| Prisma / EDM / Laser | ausgeblendet |
| Zielrichtung, Zielstrahlen, Anfahren, Standpunkt-Info | ausgeblendet |
| Messart Einfach/Mittel | **neu** – Umschalter (Abschnitt 6.1) |
| Folgen | **neu** |
| PDF, DXF-Export, Fläche, Auswahl, Messdaten | unverändert |

### 5.4 Koordinatenbezug

Der DXF-Plan kann in UTM32 (mit oder ohne Zonenkennzahl 32), in Gauß-Krüger oder in einem örtlichen System
vorliegen. Deshalb bekommt das Projekt eine Einstellung **„Bezug der GNSS-Koordinaten“**:

| Einstellung | Wirkung |
|---|---|
| **Einpassung** (Vorgabe, Regelfall – Entscheidung 14.3) | GNSS-Koordinaten werden über die aktive Einpassung (Abschnitt 8) ins Plansystem überführt |
| ETRS89/UTM32, Ostwert ohne 32 | Ausnahme: GNSS-Koordinaten direkt verwenden |
| ETRS89/UTM32, Ostwert mit 32 | dto. mit 32xxxxxx |

**Einpassung als Regelfall – Folgen für den Ablauf:**

- Beim Öffnen des DXF-Viewers mit GNSS als Messquelle prüft das Programm (wie `pruefe_aufstellung_beim_start`
  beim Tachymeter), ob eine Einpassung aktiv ist. Wenn nicht: Hinweis mit den Möglichkeiten
  **„Einpassung einrichten“**, **„Vorhandene Einpassung fortsetzen“** (mit Kontrollpunkt) oder
  **„Ohne Einpassung (UTM32)“**.
- Ohne Einpassung kann gemessen werden; die Punkte werden aber als „ohne Einpassung“ gekennzeichnet (eigene
  Farbe im Plan, `EinpassungId` leer) und lassen sich später mit „Mit aktueller Einpassung neu rechnen“
  übernehmen (6.4).
- **Absteckung** ist ohne Einpassung gesperrt, außer der Bezug ist ausdrücklich auf UTM32 gestellt.
- Die Live-Position wird ohne Einpassung nur angezeigt, wenn sie im Bereich des Plans liegt (siehe
  Plausibilitätsprüfung); sonst erscheint nur der Hinweis.

**Plausibilitätsprüfung:** Liegt die erste Fix-Position mehr als 10 km außerhalb der DXF-Ausdehnung, fragt das
Programm nach („Zonenkennzahl? Gauß-Krüger-Plan? → Einpassung einrichten“). Das fängt den häufigsten Fehler
(32 vergessen / doppelt) sofort ab.

---

## 6. Messen mit GNSS

### 6.1 Messarten

| Messart | Ablauf | Anwendung |
|---|---|---|
| **Einfachmessung** | 1 Epoche, die die Kriterien (Fix, σ, PDOP, Alter, Satelliten) erfüllt | Topografie, schnelle Detailpunkte |
| **Mittelung nach Epochen** | n gültige Epochen (Vorgabe aus GNSS-Messvorgaben, z. B. 10) | Standard-Detailpunkte |
| **Mittelung nach Zeit** | mindestens t Sekunden **und** n Epochen (`Criteria::min_seconds`) | Grenz-, Anschluss-, T-Punkte |

- Die Kriterien kommen aus den GNSS-Einstellungen (`Settings::criteria()`); im DXF-Viewer kann man Epochenzahl
  und Messzeit für die laufende Sitzung überschreiben, ohne die GNSS-Einstellungen zu ändern.
- Option „Bei Fix-Verlust neu beginnen“ (`restart_on_fix_loss`) wird übernommen.
- Während der Mittelung: Fortschrittsbalken am Messen-Knopf („12/30 Epochen · 18 s“), letzter
  Ablehnungsgrund („σ Höhe zu groß“), **Abbrechen** mit Esc oder zweitem Klick.

### 6.2 Zielhöhe

- Das Feld **Zielhöhe** im DXF-Viewer ist im GNSS-Betrieb die **Lotstabhöhe** (Boden → ARP).
- Der Antennenoffset ARP → Phasenzentrum bleibt in der GNSS-Konfiguration (Antenne und Höhe).
- Es gibt **eine** Quelle: Ändert man die Zielhöhe im DXF-Viewer, wird `pole_height` der GNSS-Sitzung gesetzt
  (und umgekehrt). Die Statuszeile zeigt „Stab 2.000 + PCO 0.065“.
- Änderung der Stabhöhe nach einer Messung → Rückfrage, ob die letzte Messung korrigiert werden soll
  (wie die IH-Korrektur beim Tachymeter; Eintrag `KORREKTUR` im Messprotokoll).

### 6.3 Punktnummer, Code, Ergebnis

- Punktnummer und Code wie beim Tachymeter (Neupunktzähler, Code-Feld, Vorschlag, Doppelprüfung).
- Ergebnisanzeige nach der Mittelung: R, H, Höhe, Streuung sR/sH/sHöhe [mm], Epochen, verworfen, Dauer,
  Fix-Anteil. Überschreitet die Streuung die Grenzen → gelbe Warnung mit „Trotzdem übernehmen / Wiederholen“.
- Der Punkt erscheint sofort im Plan (Neupunktsymbol mit eigener Farbe für GNSS-Punkte).
- Die Messung wird mit allen Daten ins Messprotokoll geschrieben (Abschnitt 10).

### 6.4 Speicherung

GNSS-Punkte haben keine Polarelemente. Statt sie in die `dat.csv`-Logik (Hz/V/S) zu zwängen, bekommen sie eine
eigene Ergebnisdatei **`{Projekt}-gnss.csv`** (Entscheidung 14.2) mit:

`PunktNr;Code;R;H;Hoehe;E_UTM;N_UTM;h_ell;H_DHHN;sR_mm;sH_mm;sHoehe_mm;Epochen;Dauer_s;Stabhoehe;PCO;Hoehenbezug;EinpassungId;Zeit`

- **Rohwerte (UTM, h_ell) bleiben immer erhalten.** R/H/Höhe sind daraus abgeleitet. Wird die Einpassung
  geändert oder ergänzt, lassen sich alle GNSS-Punkte neu rechnen (Knopf „Mit aktueller Einpassung neu
  rechnen“, mit Protokolleintrag).
- Die Neupunktliste des DXF-Viewers, der Datenmanager und der KOR-/CSV-Export lesen diese Datei mit.

---

## 7. GNSS-Absteckung

### 7.1 Ablauf (wie die tachymetrische Absteckung)

1. **Abstecken** in der Toolbar (aktiv, sobald eine Position vorliegt; ohne Einpassung nur, wenn der Bezug
   UTM32 ist).
2. Punkt im Plan antippen – Fangen wie beim Messen (DXF-Punkte, Feldbuch-, Import-, Neupunkte,
   Linienendpunkte), Nummer und Höhe werden übernommen.
3. Das **Absteckfenster** öffnet sich rechts; links bleibt der Plan mit Ziel und Antenne.
4. Zum Ziel gehen, bis die Anzeige grün ist.
5. **Messen** (Kontrollmessung mit Mittelung, Vorgabe 5 Epochen) → Abweichungen dl/dq/dh, Ampel nach
   Absteckgenauigkeit aus den Rechenparametern (`absteck_toleranz_lage_mm`, `absteck_toleranz_hoehe_mm`).
6. **„Abgesteckt“** speichert, wie beim Tachymeter; danach nächster Punkt oder Schließen.

Übernommen werden aus `gui/dxf_viewer/absteckung.rs`: `Ziel`, Zoomstufen (`zoom_stufe`, `versatz_px`),
`ampel`, `Danach`, `abschliessen`, `uebernehmen`, `schreibe_protokoll`, Ausser-Toleranz-Abfrage.
Neu ist nur die Grafik und die Quelle der Ist-Position.

### 7.2 Absteckgrafik

```
┌──────────────────────────────────────────┐
│ Ziel 1023   Abstand 3.42 m   ↑ Gehrichtung│  Kopfzeile
├──────────────────────────────────────────┤
│                    ◎ 1023                 │  Zielpunkt
│                     ╎                      │
│                     ╎   Sollrichtung       │
│                    ╱                       │
│                   ╱ ← Bewegungsvektor      │
│                  ✚  Antenne (live)         │
│                 ·                           │
│               ·  Spur der letzten Meter    │
├──────────────────────────────────────────┤
│ VOR      3.38 m                            │  bezogen auf die Gehrichtung
│ RECHTS   0.52 m                            │
│ Höhe    +0.045 m Auftrag                   │
│ Nord +2.91 m   Ost +1.85 m   (absolut)     │
│ RTK FIX · σL 9 mm · σH 15 mm               │
└──────────────────────────────────────────┘
```

- **Antenne und Zielpunkt in Echtzeit** (jede Epoche).
- Automatischer Zoom wie beim Tachymeter: im Fernbereich passt der Ausschnitt so, dass Antenne und Ziel
  sichtbar sind; ab 1 m schaltet die Anzeige auf die Zielscheibe mit 1 m / 10 cm / 1 cm-Kreisen um.
- Neben „VOR/ZURÜCK“ und „RECHTS/LINKS“ immer auch die absoluten Werte Nord/Ost, falls die Gehrichtung
  unsicher ist.

### 7.3 Ausrichtung der Grafik nach dem Bewegungsvektor

- **Grundzustand: Nord oben.** Solange keine verlässliche Bewegungsrichtung vorliegt.
- **Bewegungsrichtung bestimmen:** Aus der Spur wird der Vektor vom Punkt *vor mindestens d_min Metern*
  (Vorgabe 2 m, einstellbar 1–5 m) zur aktuellen Position gebildet. Nur Epochen mit Fix oder Float.
  Glättung über eine Ausgleichsgerade der letzten Punkte (robuster als zwei Einzelpunkte).
- **Wechsel auf „Gehrichtung oben“**, sobald die Bewegung d_min überschreitet. Die Grafik dreht sich weich
  (Animation ≤ 0,3 s), nicht ruckartig.
- **Stillstand/Nahbereich:** Unter 0,1 m/s oder näher als ~1 m am Ziel wird die Richtung **eingefroren**,
  weil Rauschen und kleine Schritte sonst die Grafik drehen. Ein Schloss-Symbol zeigt das an.
- **Hysterese:** Die Richtung wird nur nachgeführt, wenn sie sich um mehr als ~10 gon ändert und die Bewegung
  über σ liegt.
- Knopf **„Nord oben“ / „Gehrichtung oben“** zum manuellen Umschalten; Wahl wird gespeichert.
- Ein Nordpfeil am Rand zeigt immer die Nordrichtung, damit man die gedrehte Grafik einordnen kann.

### 7.4 Weitere Hilfen bei der Absteckung

- **Annäherungston** (wie Parkpiepser): Tonfolge wird mit kleiner werdendem Abstand schneller, Dauerton bei
  „innerhalb Toleranz“. Abschaltbar.
- **Stabilitätsprüfung:** „Grün“ erst, wenn die Position über mehrere Epochen (Vorgabe 3) innerhalb der
  Toleranz bleibt – verhindert grüne Ausreißer.
- **Warnung**, wenn die aktuelle σ Lage größer als die halbe Absteckgenauigkeit ist („Absteckgenauigkeit mit
  dieser Lösung nicht erreichbar“).
- **Höhe**: Auftrag/Abtrag wie beim Tachymeter, sofern das Ziel eine Höhe hat und der Höhenbezug passt.

### 7.5 Protokoll

Das Absteckprotokoll (`protokolle/absteckung.rs`) bekommt statt Hz/V/S die GNSS-Werte: Soll R/H/Höhe,
Ist R/H/Höhe (Mittel), dl/dq/dh, Epochen, σ, Fix, Satelliten, PDOP, Einpassung, Höhenbezug, Ampel/Toleranz.
Ablage und Format wie bisher.

---

## 8. Einpassung (Transformation über T-Punkte)

### 8.1 Zweck

GNSS liefert ETRS89/UTM32 und eine Höhe (ellipsoidisch oder DHHN2016). Liegt der Plan in einem anderen System
(Gauß-Krüger, örtliches Baustellensystem, älterer Lageplan, verschobener DXF) oder soll das GNSS-Ergebnis
streng an örtliche Festpunkte angeschlossen werden, braucht man eine **Einpassung**. Sie ist das GNSS-Gegenstück
zur Freien Stationierung: Statt Standpunkt und Orientierung wird eine **Transformation** aus identischen
Punkten (T-Punkten) bestimmt.

Richtung: **Quelle = GNSS (UTM32, Höhe)**, **Ziel = Plansystem (R, H, Höhe)**. Jede Live-Epoche und jede
Messung wird mit `koord_transformation::transformiere()` ins Plansystem gerechnet. Eine Umkehrtransformation
ist nicht nötig, weil Absteckung und Anzeige im Plansystem rechnen.

### 8.2 Ablauf

1. Toolbar **„Einpassung“** → Einpassfenster rechts (Aufbau wie Freie Stationierung).
2. **Punkt in der Grafik antippen** → Nummer, R, H, Höhe (Soll, Plansystem) werden in die Tabelle übernommen.
   Alternativ Nummer eintippen oder aus Importpunkten wählen.
3. Zum Punkt gehen (Hilfe: Bei bereits bestimmter Näherung zeigt eine kleine Zielscheibe den Weg, siehe 8.6).
4. **„T-Punkt messen“** → GNSS-Mittelung mit den T-Punkt-Vorgaben (Rechenparameter, z. B. 30 Epochen / 60 s).
5. Das Messergebnis (UTM, h, σ) wird als Ist dem Soll zugeordnet – Zeile wird **T-Punkt**.
6. Ab der Mindestanzahl (abhängig von der Methode) rechnet das Programm nach jeder Messung **automatisch** neu
   und zeigt Parameter, s0 und Residuen.
7. Residuentabelle wie in `gui/koord_transformation.rs`: vR, vH, vHöhe, vGesamt mit Ampel
   (1 / 3 / 9 cm aus den Rechenparametern), Häkchen **Lage aktiv** und **Höhe aktiv** je Punkt.
8. **„Übernehmen“** → Einpassung wird aktiv, gespeichert und im Messprotokoll vermerkt. Ab jetzt laufen
   Live-Position, Messen und Absteckung im Plansystem.

### 8.3 Tabelle

| Aktiv L | Aktiv H | Nr | R soll | H soll | Höhe soll | E GNSS | N GNSS | h GNSS | σL | vR | vH | vHöhe | v |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|

- Punkt löschen, erneut messen (die neue Messung ersetzt die alte, die alte steht im Protokoll),
  **Doppelmessung**: zweite Messung desselben T-Punkts wird gemittelt und die Differenz angezeigt.

### 8.4 Transformationsmethoden

Die Auswahl im Einpassfenster ist mit dem Wert aus den Rechenparametern vorbelegt (Abschnitt 9).
Grundlage ist `rechnen/koord_transformation.rs`:

| Methode | Parameter | Min. T-Punkte | Wann sinnvoll |
|---|---|---|---|
| **Verschiebung** (neu, Sonderfall) | 2 (+1 Höhe) | 1 | Plan in UTM32, nur Lagerungsfehler/Kontrolle; Notbehelf |
| **2D-Helmert, fester Maßstab** | 3 | 2 | örtliches System, Maßstab bekannt (siehe 8.5) |
| **2D-Helmert** (Ähnlichkeit) | 4 | 2 (sinnvoll ≥ 3) | **Standard** für Lageeinpassung in örtliche Systeme |
| **3D-Helmert / 7-Parameter Bursa-Wolf** | 7 | 3 | Lage und Höhe gemeinsam, größere Gebiete |
| **6-Parameter** (fester Maßstab) | 6 | 3 | wie 7-Parameter, Maßstab 1 |
| **9-Parameter** | 9 | 3 (sinnvoll ≥ 4) | getrennte Maßstäbe, z. B. verzerrte Altpläne |

**Lage und Höhe getrennt – Vorgabe (Entscheidung 14.4):** Für kleine Gebiete (Baustelle, Grundstück) ist es
geodätisch sauberer, die **Lage per 2D-Helmert** und die **Höhe getrennt** einzupassen:

| Höheneinpassung | Parameter | Min. Punkte |
|---|---|---|
| keine (Höhe wie GNSS-Höhenbezug) | 0 | 0 |
| Höhenverschiebung (konstante Anomalie) | 1 | 1 |
| geneigte Ebene (Verschiebung + Neigung N/O) | 3 | 3 |

Damit beeinflussen Höhenfehler die Lage nicht und umgekehrt; das entspricht der üblichen Praxis bei
GNSS-Baustelleneinpassungen. Die Rechnung nutzt für die Lage `TransformationsTyp::Helmert2D`, für die Höhe eine
kleine Ausgleichung (neu, in `rechnen/hoehen_einpassung.rs`). In den Rechenparametern ist daher
„2D-Helmert + Höhenverschiebung“ voreingestellt; die 3D-Methoden bleiben wählbar.

### 8.5 Maßstab und UTM-Verzerrung

UTM-Koordinaten sind verzerrt (k ≈ 0,9996 am Mittelmeridian, in Bayern ca. −400 … +200 ppm, dazu die
Höhenreduktion ≈ −h/R). Ein örtliches System hat meist Maßstab 1 in Höhe des Geländes.

- Bei **freiem Maßstab** schluckt die Ausgleichung diese Verzerrung – der Maßstab wird mit ausgegeben
  (in ppm) und plausibilisiert: Weicht er mehr als z. B. 50 ppm von der erwarteten UTM-Verzerrung am Ort ab,
  gibt es einen Hinweis (Hinweis auf falschen T-Punkt oder falsches System).
- Bei **festem Maßstab** schlägt das Programm als Vorgabe den **UTM-Maßstab am Schwerpunkt inkl.
  Höhenreduktion** vor (aus `geo::geo_to_utm().k` und der Höhe), nicht 1. Sonst entstehen auf 100 m schnell
  4 cm Fehler. Der Wert kommt aus derselben Quelle wie `polar_massstab_quelle = "UTM"`.

### 8.6 Kontrollen bei der Einpassung

- **Kontrollpunkt**: Ein Punkt mit Soll wird gemessen, geht aber **nicht** in die Berechnung ein; angezeigt
  wird nur die Abweichung (analog „Kontrolle“ beim Standpunkt). Empfohlen nach jeder Übernahme und zu Beginn
  jedes neuen Arbeitstages.
- **Geometrieprüfung**: Warnung, wenn die T-Punkte fast auf einer Linie liegen oder das Messgebiet nicht
  umschließen.
- **Extrapolationswarnung**: Liegt die Antenne weiter als ein einstellbarer Puffer (Vorgabe 20 % der
  Ausdehnung) außerhalb der konvexen Hülle der T-Punkte, zeigt die Statuszeile „außerhalb der Einpassung“
  (gelb).
- **Näherung ab dem 2. Punkt**: Nach zwei T-Punkten liegt eine vorläufige Transformation vor. Damit kann
  für den 3. und jeden weiteren T-Punkt die Absteckgrafik als Suchhilfe verwendet werden.
- **Grober Fehler**: Ein Punkt mit Residuum > 3 · s0 (und > Ampel-Rot) wird markiert; Vorschlag „Punkt
  deaktivieren und neu berechnen“.

### 8.7 Verwaltung und Speicherung

- Eine Einpassung hat eine **Id** (`E-JJJJ-MM-TT-n`) und einen Namen. Gespeichert als
  `Einpassung-<Id>.json` im Projektordner (wie `Station-*.json`): Methode, Parameter, T-Punkte (Soll, Ist roh,
  σ, aktiv), Residuen, s0, Kontrollen, Zeit.
- Mehrere Einpassungen je Projekt möglich; eine ist aktiv. Wechsel → Eintrag im Messprotokoll.
- **„Einpassung fortsetzen“** am nächsten Tag: vorhandene Einpassung laden, Kontrollpunkt messen,
  weiterarbeiten.
- Protokoll: RTF-Protokoll über `protokolle/transformation.rs` (Parameter, Residuen, Ampel), ergänzt um GNSS-
  Angaben je T-Punkt (Epochen, σ, Fix, Mountpoint, Zeit).
- Die T-Punkte werden zusätzlich im Plan dargestellt (wie `zeichne_transformationspunkte`), mit Residuenpfeil
  im überhöhten Maßstab (zuschaltbar).

---

## 9. Rechenparameter: neue Gruppe „GNSS“

| Parameter | Vorgabe | Bemerkung |
|---|---|---|
| Einpassung: Methode Lage | 2D-Helmert (4 Parameter) | Auswahl aus 8.4 |
| Einpassung: fester Maßstab | aus | bei „ein“: Vorschlag UTM-Maßstab am Ort |
| Einpassung: Höhe | Höhenverschiebung | keine / Verschiebung / geneigte Ebene / mit 3D-Methode |
| Ampelgrenzen Residuen [cm] | 1 / 3 / 9 | wie Koordinatentransformation |
| T-Punkt: Epochen / Mindestdauer | 30 / 60 s | strenger als Detailpunkte |
| Kontrollpunkt: Grenze Lage/Höhe [mm] | 30 / 50 | Ampel bei der Kontrolle |
| Extrapolationspuffer | 20 % | 8.6 |
| Absteckung: Kontrollmessung Epochen | 5 | 7.1 |
| Absteckung: Mindestbewegung für Ausrichtung | 2,0 m | 7.3 |
| Absteckung: Stabilität (Epochen in Toleranz) | 3 | 7.4 |
| Absteckung: Annäherungston | ein | 7.4 |

Absteckgenauigkeit Lage/Höhe bleibt **gemeinsam** mit dem Tachymeter (`absteck_toleranz_lage_mm`,
`absteck_toleranz_hoehe_mm`). Die eigentlichen GNSS-Messkriterien (σ-Grenzen, PDOP, Alter, Satelliten) bleiben in
der GNSS-Konfiguration (Reiter Konfiguration → Messvorgaben), damit Reiter „Messen“ und DXF-Viewer gleich messen.

---

## 10. Messprotokoll und Berechnungsprotokolle

### 10.1 GNSS-Spalten belegen und erweitern

Die vorhandenen Spalten werden belegt:

| Spalte | Inhalt |
|---|---|
| GNSS_Breite / GNSS_Laenge | Mittel, Grad, 9 Nachkommastellen |
| GNSS_Hoehe_m | ellipsoidische Höhe (Phasenzentrum) |
| GNSS_GenLage_m / GNSS_GenHoehe_m | Streuung der Mittelung (bei Einfachmessung σ aus Kovarianz) |
| GNSS_Fix | RTK_Fixed / RTK_Float / … |
| GNSS_Sat / GNSS_HDOP / GNSS_PDOP | Mittel bzw. schlechtester Wert der Belegung |

Neue Spalten **am Ende** (Formatversion 1 → 2, alte Dateien bleiben lesbar):

`GNSS_E;GNSS_N;GNSS_H_DHHN;GNSS_Hoehenbezug;GNSS_Zeta;GNSS_Epochen;GNSS_Verworfen;GNSS_Dauer_s;GNSS_Alter_s;GNSS_Mountpoint;GNSS_RefStation;GNSS_Stab_m;GNSS_PCO_m;GNSS_Zeit_GPS;EinpassungId`

- `R`, `H`, `Hoehe` enthalten wie bisher die **Projektkoordinaten** (nach Einpassung).
- `ZH_m` = Stabhöhe, `Hz/V/S/IH` bleiben leer, `Typ` = `GNSS`, `Quelle` = `Geraet`.
- `Roh`: Kurzform der GGA/PVT-Werte der letzten Epoche (zur Rekonstruktion).

### 10.2 Neue Ereignisse

| Ereignis | Wann | Inhalt |
|---|---|---|
| `GNSS_VERBINDUNG` | Verbinden/Trennen, NTRIP Start/Stop | Port, Mountpoint, Empfänger-Firmware |
| `GNSS_FIX` | Fix erreicht/verloren | Dauer bis Fix, Anzahl Verluste |
| `MESSUNG` (Typ GNSS) | jede übernommene Messung | alle Spalten aus 10.1 |
| `EINPASSUNG` | Übernahme/Wechsel/Löschen | Id, Methode, Parameter, s0, T-Punkte |
| `KONTROLLE` (Typ GNSS) | Kontrollpunkt | Soll, Ist, Abweichung, Ampel |
| `ABSTECKUNG` | „Abgesteckt“ | wie Tachymeter, GNSS-Spalten belegt |
| `KORREKTUR` | Stabhöhe nachträglich geändert | Alt/Neu |

### 10.3 Weitere Protokolle

- Einpassung → `protokolle/transformation.rs` (RTF), erweitert um GNSS-Angaben.
- Absteckung → `protokolle/absteckung.rs` (7.5).
- Die GNSS-eigenen Dateien (`-protokoll.csv` des Messen-Reiters) bleiben unverändert erhalten.

---

## 11. Dateien und Datenmodell

| Datei | Neu/geändert | Inhalt |
|---|---|---|
| `{Projekt}-gnss.csv` | neu | GNSS-Punkte mit Rohwerten und Projektkoordinaten (6.4) |
| `Einpassung-<Id>.json` | neu | Einpassung (8.7) |
| `{Projekt}-Messprotokoll.csv` | erweitert | GNSS-Spalten, neue Ereignisse (10) |
| `GNSS\*.ubx` | neu, optional | Rohdatenaufzeichnung je Sitzung (12.6) |
| `%LOCALAPPDATA%\Feldbuch\gnss.toml` | neu | GNSS-Konfiguration des Feldbuch-Zweigs (2.5) |
| `%APPDATA%\gnss-vermessung\settings.toml` | unverändert, nur gelesen | einmalige Übernahme (2.5) |
| `src/gnss/HERKUNFT.md` | neu | Herkunftsvermerk des übernommenen Codes (2.3) |
| Projekteinstellungen | erweitert | Gerätetyp `GnssSapos`, Koordinatenbezug, aktive Einpassung |
| Rechenparameter | erweitert | Gruppe GNSS (9) |

Neue Module (Vorschlag):

```
src/gnss/kern/, src/gnss/ansicht/ – übernommener Code (2.2)
src/gnss/mod.rs              – Sitzung, LivePosition, Messquelle
src/gnss/einpassung.rs       – Datenmodell, Berechnung (nutzt rechnen/koord_transformation), Speicherung
src/gnss/bewegung.rs         – Bewegungsrichtung aus der Spur (Glättung, Hysterese, Einfrieren)
src/rechnen/hoehen_einpassung.rs – Höhenverschiebung / geneigte Ebene
src/gui/gnss_ansicht.rs      – Ansicht::Gnss (zeichnet gnss::ansicht::App)
src/gui/dxf_viewer/gnss.rs   – Live-Position, Statuszeile, Messen im GNSS-Betrieb
src/gui/dxf_viewer/gnss_absteckung.rs – Absteckgrafik mit Gehrichtung
src/gui/dxf_viewer/einpassung.rs     – Einpassfenster
```

---

## 12. Eigene Vorschläge und Ideen

### 12.1 Fix-Wächter
Der Messen-Knopf ist nur bei RTK Fix grün. Bei Float ist er gelb und fragt nach („Float – trotzdem messen?“,
nur Einfachmessung). Fix-Verlust während einer Mittelung → Ton und deutliche Meldung. Die Statuszeile zeigt die
Dauer seit dem letzten Fix; Messungen kurz nach einer Neuinitialisierung (< 10 s) werden markiert, weil dort
falsche Mehrdeutigkeitslösungen am häufigsten sind.

### 12.2 Zweite unabhängige Belegung
Für wichtige Punkte (T-Punkte, Grenzpunkte) optional eine **zweite Belegung** nach einer Mindestzeit
(z. B. 10 min) mit vorheriger **Neuinitialisierung** (Abschattung der Antenne oder Empfänger-Reset per UBX).
Vergleich beider Belegungen mit Grenzwert, Mittel als Ergebnis, alles im Protokoll. Das ist die übliche
Absicherung bei SAPOS-HEPS-Messungen und deckt falsche Fix-Lösungen auf, die keine Streuung zeigt.

### 12.3 Kontrolle am Sitzungsbeginn
Nach dem Verbinden schlägt das Programm vor, einen bekannten Punkt (Festpunkt, T-Punkt der aktiven Einpassung)
als Kontrolle zu messen. Ergebnis mit Ampel im Protokoll.

### 12.4 Exzentrische Punkte (Abschattung)
Hauswand, Baum, Zaun: GNSS kommt nicht an den Punkt. Lösungen mit vorhandenen Rechenfunktionen:
- **Exzentrum Längs/Quer** bezogen auf zwei GNSS-Hilfspunkte (Gerade) – „Punkt in 1,25 m Verlängerung,
  0,30 m rechts“.
- **Bogenschnitt** aus zwei GNSS-Hilfspunkten und zwei Bandmaßstrecken (`rechnen/bogenschnitt.rs`).
- **Geradenschnitt** zweier Gebäudekanten aus je zwei GNSS-Punkten.

### 12.5 GNSS und Tachymeter kombinieren
- GNSS-Punkte als **Anschlusspunkte für die Freie Stationierung**: zwei bis drei GNSS-Punkte im offenen Gelände
  messen, Tachymeter frei stationieren, im Hof/Gebäude weitermessen – alles im gleichen System.
- Umgekehrt: Einpassung mit Punkten, die vorher tachymetrisch bestimmt wurden.
- Dazu muss die Messquelle im DXF-Viewer schnell umschaltbar sein (Abschnitt 4.4), ohne die Sitzung zu beenden.

### 12.6 Rohdatenaufzeichnung je Sitzung
Die Engine kann bereits aufzeichnen (`start_recording`). Vorschlag: Option „UBX-Rohdaten im Projekt
mitschreiben“ (`<Projekt>\GNSS\JJJJ-MM-TT_hhmm.ubx`). Nutzen: Beweissicherung, Wiedergabe der Sitzung im
Büro, spätere Nachbearbeitung (mit RXM-RAWX auch RINEX).

### 12.7 Wiedergabe als Schulungs- und Testmodus
Die vorhandene Wiedergabe (`--replay demo.ubx`) wird im Feldbuch als „Simulation“ angeboten. Messungen sind dann
deutlich als Simulation gekennzeichnet und landen in einer eigenen Datei (wie in gnss-gui), nie im Projekt.
Ideal, um Absteckung und Einpassung am Schreibtisch zu testen.

### 12.8 Linien- und Stop-and-go-Aufnahme
Automatische Punktaufnahme alle x m oder x s (z. B. Bordsteinkanten, Wege, Geländeprofile) mit fortlaufender
Nummer und Code; die Linie erscheint sofort im Plan.

### 12.9 Höhenbezug sichtbar und erzwungen
Bei der ersten GNSS-Messung eines Projekts Abfrage des Höhenbezugs (ellipsoidisch / DHHN2016 über SAPOS /
Quasigeoid-Datei / Höhenanschluss). Das verhindert gemischte Höhen im Projekt. Der Höhenbezug steht in jeder
Zeile des Messprotokolls.

### 12.10 Höhenanschluss an einen Höhenfestpunkt
Sonderfall der Höheneinpassung: ein Höhenfestpunkt messen → Höhenanomalie ζ wird bestimmt und als
„konstante Höhenanomalie“ der Sitzung gesetzt (entspricht dem Ausbaupunkt „Höhenanschluss“ in
gnss-vermessung, Plan 12).

### 12.11 Qualitätskarte im Plan
Die Spur im DXF-Viewer zeigt farbig, wo Fix, Float oder kein Fix war. Nach einem Messtag sieht man sofort die
Abschattungsbereiche – nützlich für die Planung (Tachymeter statt GNSS).

### 12.12 GPS-Zeit statt PC-Zeit
Für das Protokoll wird zusätzlich die GNSS-Zeit (UTC aus TIM/NAV) gespeichert. Die PC-Uhr des Feldrechners ist
oft ungenau; die GNSS-Zeit ist ein unabhängiger, belastbarer Zeitstempel.

### 12.13 Satellitenvorhersage vor dem Außendienst
Aus den gespeicherten Bahndaten (Reiter Satelliten/DOP) eine einfache Vorhersage „wann ist PDOP am
Messort schlecht“ – hilft bei engen Lagen (Hauswände, Wald).

### 12.14 Toughpad-Bedienung
Große Knöpfe in Absteckfenster und Einpassung, Messen auch über die Leertaste/Hardwaretaste, Annäherung per Ton,
damit man beim Gehen nicht ständig auf den Bildschirm sehen muss.

---

## 13. Umsetzungsschritte

| Schritt | Inhalt | Aufwand |
|---|---|---|
| 1 | Code von gnss-vermessung 0.4.0 nach `src/gnss/` kopieren (ohne CLI und Update), `HERKUNFT.md`, Abhängigkeiten, übernommene Tests grün; Oberfläche als einbettbare Ansicht, Schnittstelle `LivePosition`/Messung, eigene Einstellungen mit einmaliger Übernahme – **gnss-vermessung selbst bleibt unberührt** | M |
| 2 | Feldbuch: `Ansicht::Gnss`, Knopf neben DXF-Viewer, Sitzung in `FeldbuchApp`, Statuslampe, dauerhafte Empfängerkonfiguration mit Rücklesevergleich | K |
| 3 | Messgerät-Kommunikation: Umbenennung, Gerätetyp `GnssSapos`, `GnssNmea` und `tachymeter/gnss.rs` entfernen, Umstellung alter Projekte, GNSS-Gruppe, Messquelle | K |
| 4 | DXF-Viewer: Live-Position, Spur, Fehlerellipse, Folgen, Statuszeile, Toolbar je Messquelle, Bezugssystem (Vorgabe Einpassung), Startprüfung, Plausibilitätsprüfung | M |
| 5 | GNSS-Messen: Einfach/Mittelung, Stabhöhe, `{Projekt}-gnss.csv`, Messprotokoll Formatversion 2 | M |
| 6 | Einpassung: Fenster, T-Punkte aus Grafik, Methoden, Höheneinpassung, Kontrollpunkt, Speicherung, Protokoll | G |
| 7 | GNSS-Absteckung: Grafik, Bewegungsrichtung, Ton, Stabilität, Protokoll | M |
| 8 | Rechenparameter-Gruppe GNSS, Anleitung, Tests (Bewegungsrichtung, Einpassung gegen Koordinatentransformation, Protokoll-Rundlauf) | M |
| 9 | Ideen aus Abschnitt 12 nach Priorität | – |

Empfohlene Reihenfolge der Ideen: 12.1 Fix-Wächter → 12.9 Höhenbezug → 12.3 Kontrolle → 12.6 Rohdaten →
12.4 Exzentrum → 12.2 Zweitbelegung → 12.5 Kombination → Rest.

Tests mit der Wiedergabe (`demo.ubx`) ohne Empfänger: Live-Position, Messen, Absteckung und Einpassung lassen
sich so automatisiert prüfen.

---

## 14. Entscheidungen (26.11.2026)

| Nr | Frage | Entscheidung | Eingearbeitet in |
|---|---|---|---|
| 14.1 | Bisheriger Gerätetyp „GNSS (NMEA 0183)“ | **entfällt**; alte Projekte werden auf GNSS-SAPOS umgestellt | 4.2, 13 |
| 14.2 | Ablage der GNSS-Punkte | **eigene Datei `{Projekt}-gnss.csv`** | 6.4, 11 |
| 14.3 | Koordinatenbezug | **Einpassung ist der Regelfall** (Vorgabe), UTM32 direkt nur als Ausnahme | 5.4, 13 |
| 14.4 | Höheneinpassung | **getrennt** (2D-Helmert + Höhenverschiebung/Ebene) als Vorgabe, 3D-Methoden wählbar | 8.4, 9 |
| 14.5 | Reiter „Messen“ der GNSS-Ansicht | **bleibt eigenständig nutzbar** (eigene Dateien, Vorgabe Projektordner) | 3 |
| 14.6 | Pflege und Updates | **gnss-vermessung bleibt unverändert** und wird weiter eigenständig gepflegt (mit eigener Update-Funktion). Der GNSS-Teil im Feldbuch ist eine **eigene Kopie**, unabhängig gepflegt und nur über Feldbuch-NT aktualisiert | 1.1, 2, 11, 13 |
| 14.7 | Empfängerkonfiguration | **dauerhaft im Empfänger** (RAM + BBR + Flash, nur bei Änderung geschrieben) | 2.6, 13 |

Keine offenen Fragen mehr; die Umsetzung kann mit Schritt 1 (Abschnitt 13) beginnen.

---

## 15. Umsetzungsstand (26.11.2026, Version 0.10.1, Build 55)

Geprüft mit 503 automatischen Tests (darunter die übernommenen Tests des GNSS-Programms und eine Ende-zu-Ende-Prüfung
mit der Wiedergabe von `testdaten/gnss/demo.ubx`). **Mit Empfänger und in der Oberfläche noch nicht geprüft.**

### Umgesetzt

| Abschnitt | Stand |
|---|---|
| 2 Einbindung | Code von gnss-vermessung 0.4.0 in `src/gnss/kern` und `src/gnss/ansicht`, Herkunft in `src/gnss/HERKUNFT.md`; eigene Einstellungen mit einmaliger Übernahme; Tresor-Dienst `feldbuch-nt-sapos`; Update-Funktion entfernt; dauerhafte Empfängerkonfiguration mit Flash-Vergleich |
| 3 Hauptmenü | Knopf „GNSS-Vermessung“ neben dem DXF-Viewer mit Statuslampe; das GNSS-Fenster läuft weiter, wenn es geschlossen wird |
| 4 Messgerät-Kommunikation | Umbenennung, Gerätetyp „GNSS – SAPOS (u-blox ZED-F9P)“, alter NMEA-Typ entfernt und umgestellt, GNSS-Gruppe mit Verbinden/Trennen und „GNSS-Fenster öffnen“ |
| 5 DXF-Viewer | Antenne mit Fehlerellipse, Spur farbig nach Lösung, Bewegungspfeil, „Folgen“, Statusanzeige, Werkzeugleiste ohne Tachymeterfunktionen, Koordinatenbezug, Startfrage ohne Einpassung, Plausibilitätsprüfung (> 10 km) |
| 6 Messen | Einfachmessung und Mittelung, Stabhöhe aus dem Feld „Stab“ (gleicht die GNSS-Einstellung ab), Fix-Wächter, `{Projekt}-gnss.csv`, Nummer weiterzählen, Messprotokoll |
| 7 Absteckung | eigenes Fenster mit Grafik, Ausrichtung nach Gehrichtung (Mindestweg, Glättung, Hysterese, Einfrieren unter 0,1 m/s und unter 1 m), Vor/Rechts bzw. Nord/Ost, Ringe 1 m / 10 cm / 1 cm, Ampel, Stabilität, Kontrollmessung, „Abgesteckt“, RTF-Absteckprotokoll |
| 8 Einpassung | Fenster mit T-Punkten aus dem Plan, Methoden aus 8.4, Höhe getrennt, Mehrfachbelegung gemittelt, Residuen mit Ampel und Aktiv-Schaltern, Kontrollpunkt, Geometrie- und Maßstabshinweis, Probe auf groben Fehler (Punkt weglassen), Rahmenwarnung, Übernahme, RTF-Protokoll, „GNSS-Punkte neu rechnen“ |
| 9 Rechenparameter | Gruppe „GNSS – Einpassung und Absteckung“ |
| 10 Messprotokoll | GNSS-Spalten belegt, Ereignisse `GNSS_VERBINDUNG` und `EINPASSUNG`, Typen `GNSS`, `GNSS-TP`, `GNSS-KP`, `GNSS-AB` |

### Abweichungen vom Vorschlag

- **GNSS als eigenes Fenster** statt als Ansicht im Hauptfenster (3): Das Hauptfenster wird beim DXF-Viewer
  minimiert; so bleiben GNSS-Überwachung und Plan nebeneinander sichtbar.
- **Messprotokoll ohne neue Spalten** (10.1): Die zusätzlichen Angaben (UTM, Höhenbezug, Einpassung, Epochen,
  Mountpoint, Referenzstation, Korrekturalter, Grenzen …) stehen als `Schlüssel=Wert` in der Spalte „Roh“. So bleibt
  die Datei nur anhängend und Formatversion 1 gültig.
- **Einpassungen in einer Datei** `Einpassung.json` im Projektordner (Liste und aktive Id) statt je Einpassung eine Datei.
- **Rahmen statt konvexer Hülle** für die Warnung „außerhalb des Bereichs der T-Punkte“ (8.6).
- **Annäherungston** (7.4): ein Ton, sobald die Position stabil in der Toleranz ist; keine Tonfolge nach Abstand.
- **Wiedergabe** (12.7): Punkte aus einer Wiedergabe landen im Projekt, sind aber mit Quelle `WIEDERGABE` und im
  Messprotokoll gekennzeichnet.
- **RTF-Protokoll der Einpassung**: Die Parameter beziehen sich auf die Schwerpunkte (Reduktion für eine stabile
  Ausgleichung mit UTM-Koordinaten).

### Noch offen

- Rückfrage „Letzte Messung mit neuer Stabhöhe korrigieren?“ (6.2) und „Trotzdem übernehmen / Wiederholen“ bei zu
  großer Streuung (6.3).
- GNSS-Punkte in Datenmanager und KOR-/CSV-Export (6.4); im Plan erscheinen sie bereits.
- Ideen aus Abschnitt 12 außer Fix-Wächter, Qualitätskarte (Spur) und Wiedergabe.
- Prüfung im Feld: Verbinden, dauerhafte Konfiguration, Fix, Messen, Einpassung auf einem Festpunktfeld, Absteckung.

