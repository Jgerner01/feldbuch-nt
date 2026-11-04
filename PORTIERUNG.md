# Feldbuch-NT – Stand der Portierung (C# → Rust)

Quelle: `..\Feldbuch` (C#, WinForms, ca. 26.000 Zeilen Logik, 31 Formulare)
Ziel:   Rust mit egui/eframe, eine einzelne EXE

## Umgesetzt und getestet (64 Unit-Tests)

| C#-Datei(en) | Rust-Modul | Anmerkung |
|---|---|---|
| AppPfade, DateiSpeicher, ErrorLogger, ProtokollManager | `grundlagen/` | atomares Schreiben, Schreibsperre nach Ladefehler, Backup |
| ProjektManager | `daten/einstellungen.rs` | `Einstellungen.xml` kompatibel, Migration alter Modellnamen |
| RechenparameterManager | `daten/rechenparameter.rs` | `Freie-Stationierung.xml` kompatibel |
| ProjektdatenManager | `daten/projektdaten.rs` | `Projektdaten.csv` kompatibel |
| Feldbuchpunkte-, Import-, NeupunkteManager | `daten/punktlisten.rs` | JSON im Format der C#-Version |
| StationsdatenManager, AnschlusspunktSuche, FestpunktUebernahme | `daten/projekt.rs` | eine `Projekt`-Struktur statt statischer Manager |
| FreieStationierung, Polarpunkt-/NeupunktRechner | `rechnen/` | |
| BogenschnittRechner | `rechnen/bogenschnitt.rs` | |
| HochpunktherablegungRechner | `rechnen/hochpunkt.rs` | |
| DxfParser (Leser, Entities), DxfOverlayEntity | `dxf/` | gleiche unterstützte Entities, ACI-Farben, Symbole |
| DxfPunktIndex, DxfPunktIndexManager | `dxf/punktindex.rs` | Nummern bleiben über erneutes Laden erhalten |
| FormDxfViewer, DxfCanvas | `gui/dxf_viewer*` | eigenes Fenster, siehe unten |
| DataGridView-Optik | `gui/tabelle.rs` | Rasterlinien, graue Köpfe, AliceBlue-Zeilen |
| Hauptfenster, Projekt, Projektdaten, Rechenparameter, Freie Stationierung | `gui/` | |

### DXF-Viewer (Nachbau)
Toolbar (blau), Seitenleiste (dunkel, mit den Original-Icons), Statusleiste mit Ebenen-Kontrollkästchen,
weiße Zeichenfläche mit Fadenkreuz, Zoom (Mausrad ×1,25, Knöpfe ×1,5), Pan, Snap (orange Quadrat),
Picking, Katasterpunkt-Symbole, Punktnummern-Marker, Overlays (Standpunkt, Neupunkt, Import, gemessene
Neupunkte, Residuen, Fehlerellipsen mit Maßstabsbalken), Signallampe, lokales System, Import KOR/CSV/JSON
mit Begleit-CSV-Erkennung, Digitalisierung (.kor und -kor.csv), Löschen per Fenster, Punkt-Index
zurücksetzen, Festpunkte übernehmen, Zoom-Speicherung je DXF.

### Tachymeter-Schicht (ohne Gerät getestet, 107 Tests insgesamt)
Parser für GeoCOM, GSI-8/16, GSI Online, Sokkia SDR und Topcon gegen Beispielzeilen; Befehle je Modell;
Messdatenmodell; Konfiguration (Prisma/RL) mit Abgleich samt Transaktionsnummern; serielle Verbindung
(Hintergrund-Leser, mehrere Abonnenten, Senden mit Wiederholung); Messungscache; Quittungstöne;
Sperre der Live-Winkelabfrage; Fenster „Messgerät – Kommunikation" mit Empfangsprotokoll und Testbefehl.
**Nicht mit echter Hardware geprüft.**
Serielle Verbindung: ein einziger Ein-/Ausgabe-Thread besitzt den Port (Senden und Lesen im Wechsel, 25 ms
Lese-Timeout), weil sich unter Windows Lesen und Schreiben auf einem gemeinsamen synchronen Handle
gegenseitig blockieren. DTR/RTS werden ausdrücklich gesetzt (Standard aus, wie .NET), Zeilenende wählbar,
Hex-Ansicht der Rohbytes und eine Verbindungsanalyse (Baudraten × Zeichenformate × DTR/RTS).
Geräte: Leica TS06, TPS300-Serie, TPS1200 (GeoCOM 38400/9600), TS11, Trimble S3, Topcon, GNSS, manuell.
Sokkia SET ist entfernt (alte Einstellungen fallen auf „Manuell"). Bei den Leica-Geräten ist zwischen GeoCOM und
GSI Online umschaltbar (TS06 Standard GeoCOM, TPS300 Standard GSI Online). Alle Wartezeiten sind auf höchstens
10 s begrenzt: Öffnen des Ports (im Hintergrund, Fenster bleibt bedienbar), jeder Schritt der Testmessung.
Kommunikationsprotokoll: `%LOCALAPPDATA%\Feldbuch\Kommunikation.log` (Zeitstempel, gesendet/empfangen immer auch
als Hex; bei > 2 MB gerollt nach Kommunikation.alt.log).
Schalter-Buttons „Reflektor/Reflektorlos" und „Laserpointer": Befehle werden nacheinander gesendet, jede Quittung
des Geräts wird geprüft (GeoCOM: Returncode, GSI Online: „?"), Fehler erscheinen im Klartext; der Laserpointer
wird beim Trennen ausgeschaltet.
Testmessung: GeoCOM = Winkel lesen → Messung auslösen → Ergebnis holen; GSI Online = GET/M.
Abweichungen vom Original: 1,5 Stoppbits kennt die serielle Bibliothek nicht (es wird 2 verwendet);
GeoCOM-Antworten ohne Kontext werden nach Datenanzahl gedeutet (im Original unerreichbar);
Liste gekoppelter Bluetooth-Geräte (WMI) fehlt, es gibt stattdessen die COM-Port-Liste und den Weg
zu den Windows-Bluetooth-Einstellungen.

Messablauf im Viewer (Messen-Button): GeoCOM-Folge Winkel → Messung → Ergebnis mit Zusatzdaten (je Schritt höchstens
10 s, Zusatzschritte 3 s), GSI Online einstufig. Das Ergebnis wird wie im Original verarbeitet (Tagesprotokoll,
Messdatei, Neupunktberechnung bzw. Stationierung mit Residuen, Neuberechnung der Neupunkte, Standpunktinfo,
Konfigurationswarnung, Exzentrizität zurücksetzen). Zielrichtung: alle 500 ms Hz-Abfrage, rot gestrichelte
Linie mit „Hz … t …"; die Abfrage ruht während Messung und Schaltvorgängen. EDM- und Laser-Button im Viewer
steuern das Gerät. Mit TS06 getestet: Verbindung, Testmessung, Reflektor/Reflektorlos, Laserpointer;
Messen im Viewer und Zielrichtung noch ungetestet.

## Noch nicht portiert

- Polarpunkte sind portiert (`gui/polarpunkte.rs`, `daten/polar_messdaten.rs`, `daten/kor_csv.rs`, 181 Tests): Lesen der
  Messdatei (alt/neu), Orientierung aus gespeicherter Stationierung oder neu aus den Anschlusspunkten, Ein-Ziel-Fall,
  lokales System, Ampel mit schaltbaren Beobachtungen, Exzentrizitäten je Punkt (Rückschreiben in die dat.csv),
  Maßstab nach Rechenparameter, Rückfrage vor dem Überschreiben, Speichern nach Neupunkte.json, KOR-CSV mit Ergänzen.
  Es fehlt der Knopf „Rechenparameter" (die Parameter ändert man im Hauptmenü, dann „Neu berechnen").
- Datenmanager ist portiert (`gui/daten_editor.rs`, `daten/csv_datei.rs`, 188 Tests): zwei Editoren für *-kor.csv und
  *-dat.csv mit Metadaten, Tabelle, Spaltenverwaltung, KOR-/DAT-Import und -Export, „Alle setzen / Zu allen addieren"
  für eine Spalte. Abweichung: Das Original öffnet das Spaltenmenü per Rechtsklick auf die Kopfzeile, Feldbuch-NT bietet
  Spaltenauswahl und Knöpfe darüber; die Zeilen löscht man über einen Haken statt über die Zeilenmarkierung.
  Beim Verlassen wird wie im Original gespeichert (nur bei bekanntem Dateipfad).
- Zahlenformat: `f()` rundet exakte Rundungsmitten jetzt wie .NET von null weg (1,25 → 1,3; Rust rundet auf gerade).
- Flächenteilung ist portiert (`rechnen/flaechenteilung.rs`, `gui/flaechenteilung.rs`, 171 Tests) mit allen sechs
  Verfahren, Vorschau mit Klickauswahl von Grundkante, Eckpunkt und festem Punkt. Abweichung: Beim Verfahren
  „durch festen Punkt" sucht Feldbuch-NT mit Stichproben nach einem Vorzeichenwechsel, weil die Intervallsuche des
  Originals bei einem Flächenmaximum zwischen zwei Eckpunktrichtungen versagt. Nicht erreichbare Flächen werden
  gemeldet. Es fehlt die Übergabe an die Absteckung.
- Koordinatentransformation ist portiert (`rechnen/koord_transformation.rs`, `gui/koord_transformation.rs`, 158 Tests):
  2D-Helmert (frei/fester Maßstab), 3D/7-Parameter, 9-Parameter, 6-Parameter, Passpunktzuordnung, Ampel-Residuen mit
  Aktiv- und Z-Haken, Transformation aller Punkte, KOR-/CSV-Import und -Export.
- Berechnungsformulare: Rückwärts-, Vorwärts-, Bogenschnitt und Hochpunktherablegung sind als Formulare unter
  „Berechnungen" portiert (`gui/berechnungen.rs`, Rechenkerne in `rechnen/`).
  (Abweichung: Iterationen werden korrekt gezählt, das Original zählte doppelt)
- Fenster „Messwerte / Testmessungen" (`gui/testmessungen.rs`, 226 Tests): farbiges Terminal der Kommunikation,
  Messwertanzeige, Messung auslösen, EDM-Modus dreistufig (Prisma / RL kurz / RL lang, wird gespeichert und am Gerät
  eingestellt), Winkel-Dauerübertragung (500 ms), Signaltöne (GeoCOM), Laserpointer und animierte Dosenlibelle mit
  Dreifuß (Startsequenz über die Atmosphärendaten, ICAO-Standard bei unplausiblen Werten, Bereichswarnung). Die
  Abfragen ruhen während einer Messung. Das Gerät liefert dafür die zuletzt gelesene Messung (`letzte_messung`).
- Konvertierung ist portiert (`gui/konvertierung.rs`, `daten/konvertierung.rs`): Feldbuchpunkte als Tabelle, Import aus
  GSI (Leica GSI-8/-16) und DXF (Linienendpunkte, Symbole, Punkte, Kreise) mit Duplikaterkennung und Nummernvergabe,
  Export als CSV, KOR und DAT.
- DXF-Export im Viewer (`dxf/exporter.rs`, `gui/dxf_viewer/export.rs`): AC1009 mit Layern je Kategorie und Quelle,
  Maßstab, Symbol- und Textgröße, Fehlerellipsen als Polylinien.
- Dialog „Messdaten eingeben" (`gui/dxf_viewer/messdaten_eingabe.rs`): öffnet beim Klick auf einen Punkt im Viewer;
  Messwerte von Hand oder per Tachymeter (Messen-Knopf, Prisma/Reflektorlos), Übernahme in die Anschlusspunktdatei.
  Abweichung: Der Dialog ist nicht blockierend, Klicks bei offenem Dialog öffnen aber keinen zweiten.
- RTF-Protokolle sind portiert (`protokolle/`, 206 Tests): Der Generator liest die XML-Vorlagen (jetzt in die EXE
  eingebettet, Quelle im Ordner `vorlagen/`) und unterstützt Titel, Zeile, Monoblock, Tabelle (benannt, Ampelfarben,
  bedingte Spalten), Leerzeile, Trennlinie, Farben, Querformat und bedingte Abschnitte. Es gibt Protokolle für Freie
  Stationierung, Rückwärts-/Vorwärts-/Bogenschnitt, Hochpunktherablegung, Koordinatentransformation, Flächenteilung
  (mit Lageplan als PNG, ohne Beschriftung) und Polarpunkte. Die Schnitte schreiben beim Verlassen automatisch, wie im
  Original; die anderen Formulare haben einen Knopf „Protokoll" (Polarpunkte schreiben beim Speichern).
  Die Dateien landen im Projektverzeichnis und nur bei eingeschalteter Protokollierung.
- Aufnahmetest mit Anfahren ist portiert (`gui/aufnahmetest.rs`, `daten/aufnahmetest.rs`, `tachymeter/anfahren.rs`,
  290 Tests insgesamt): Messen ins lokale System (Instrument = Ursprung), bearbeitbare Tabelle (`aufnahmetest.json`, Format
  der C#-Version), motorisches Anfahren (GeoCOM: Ist-Winkel → relative Bewegung 6007 → Ist-Winkel, Bewegung höchstens 30 s,
  Stopp-Knopf), Export als KOR und DAT. Geöffnet wird er im DXF-Viewer (Knopf ⊙ oder Rechtsklick „Anfahren: Nr“ auf einen
  Importpunkt, der in der Tabelle des Aufnahmetests vorkommen muss). **Nicht mit Hardware geprüft; das Anfahren bewegt das
  Gerät.** Abweichungen: Das Bearbeiten von X/Y/Z rechnet Strecke, Hz und V neu (im Original blieben sie veraltet, obwohl
  das Anfahren sie benutzt); „-0.0000“ wird als „0.0000“ geschrieben; der Aufnahmetest ist eine Ansicht im Hauptfenster
  statt eines eigenen Fensters. Einen NMEA-Parser gibt es auch im Original nicht, nur das Datenmodell.
- Info-Seite ist voll portiert (Icon, Version, Autor, Hilfetext).
- Programm-Icon: neues Icon (blaue Kachel, Zielkreuz mit orangen Marken, „NT“) statt „FB“. `tools\Icon-Erzeugen.ps1`
  erzeugt `icons\app_icon.png` (Fenster, Info-Seite) und `icons\app.ico` (16 bis 256 px; die kleinen Größen sind
  vereinfacht gezeichnet). `build.rs` bettet die ICO samt Produktname und Version als Windows-Ressource in die EXE ein
  (braucht den Ressourcen-Compiler rc.exe aus dem Windows SDK; fehlt er, entsteht die EXE ohne Icon und der Build
  warnt).
- Update-Dienst ist portiert (`update.rs`, `gui/update_dialog.rs`, `tools/Release-Paket.ps1`, 315 Tests insgesamt): Beim Start
  (nur in veröffentlichten Versionen, nie in `target\`) und auf Knopfdruck in der Info-Seite sucht er in den GitHub-Releases
  von `Jgerner01/feldbuch-nt` nach einer neueren Version. Ein Release braucht `Feldbuch-NT-v<Version>-win-x64.zip` und
  `…zip.sha256` (erzeugt von `tools\Release-Paket.ps1`, das nichts veröffentlicht). Das Fenster „Update verfügbar“ lädt mit
  Fortschritt und Abbruch, prüft Größe und SHA-256, entpackt (Pfade außerhalb des Ordners werden abgelehnt) und startet die
  neue EXE mit `--update-installieren <pid> <neuer Ordner> <Zielordner>`; sie wartet auf das Ende der alten Version, ersetzt nur
  Paketdateien, sichert das Alte in `_vorherige_Version`, rollt bei Fehlern zurück und startet neu. Installiert wird nur, wenn
  kein DXF-Viewer und kein anderes Modul als Hauptmenü/Info offen ist und der Programmordner beschreibbar ist.
  Getestet: Versionsvergleich, Release-Auswertung (inkl. Host- und Größenprüfung), Download/Prüfsumme/Entpacken gegen einen
  lokalen Server, Austausch mit Rollback und der Installationsmodus mit der echten EXE. **Nicht getestet: ein echtes Release
  auf GitHub** (es gibt noch keines; die API meldet 404, das gilt wie im Original als „kein Update“). Abweichungen: Die
  Schnittstelle trägt die Namen von Feldbuch-NT (Repository, `feldbuch-nt.exe`, Paketname), die Prüfsumme ist wie im Original
  keine Signatur.
- Absteckung ist portiert (`rechnen/absteckung.rs`, `gui/absteckung/`, `protokolle/absteckung.rs`, 271 Tests insgesamt):
  Punkt-, Achs-, Raster-, Profilabsteckung und Schnurgerüst mit Lageplan (eigene Zeichenfläche statt DXF-Karte),
  Einweiser (Zielscheibe mit Hz-/s-Abweichung, Handeingabe wie im Original), RTF-Protokollen (beim Verlassen und per
  Knopf; mit Lageplan-PNG außer bei der Profilabsteckung). **Noch nicht in der Oberfläche geprüft** (nur Tests und Start).
  Abweichungen vom Original:
  - Der Lageplan ist nicht die DXF-Karte: Statt Punkte aus dem DXF zu fangen, zeigt er die bekannten Projektpunkte
    (Import-, Feldbuch-, Anschlusspunkte) als Hintergrund; ein Klick darauf füllt die nächste freie Zeile (Punktabsteckung,
    Schnurgerüst vor dem Fixieren) bzw. Anfang/Ende der Achse. Mausrad zoomt, Ziehen verschiebt, Doppelklick setzt zurück.
    Ein Doppelklick auf freie Fläche fügt keine Polygonpunkte ein; Punkte aus der DXF-Datei selbst lassen sich nicht fangen.
    Außerdem: „Anschlusspunkte laden“, Klick auf Sollpunkte und Gebäudeseiten und bei der Achsabsteckung eine Punktauswahl.
  - Statuswerte „abgesteckt“ bleiben bei Neuberechnung erhalten, solange Nummer und Koordinaten gleich sind.
  - Profilabsteckung: Eine geänderte Planhöhe aktualisiert ΔH sofort (im Original blieb sie veraltet); ΔH wird im Text
    statt im Zellhintergrund gefärbt.
  - Schnurgerüst: Das Vorzeichen des Abstands zur Gebäudeseite war im Original vertauscht (außen galt als „innen“) und
    ist korrigiert. Die Messung kommt vom Tachymeter (Messen-Knopf, ungetestet) oder von Hand (Hz und s).
  - Im Protokoll der Punkt- und Achsabsteckung steht der Lageplanhinweis immer (im Original blieb er leer, weil die
    Prüfung vor dem Schreiben des Bildes lief).
  - Der Lageplan im PNG hat keine Beschriftung (wie bei der Flächenteilung).
