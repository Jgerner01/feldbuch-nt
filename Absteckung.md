# Punktabsteckung im DXF-Viewer – Vorschlag

Stand: 24.11.2026 · Status: **umgesetzt (Abschnitt 7), in der Oberfläche noch nicht geprüft**

## 1. Ablauf aus Sicht des Benutzers

1. Im DXF-Viewer den neuen Knopf **„Abstecken“** in der Toolbar wählen. Er ist nur aktiv, wenn eine
   gültige Stationierung vorhanden ist (oder das lokale System gewählt ist). Sonst erscheint der bekannte
   Hinweis „Neuer Stp …“.
2. Einen Punkt in der Grafik antippen. Gefangen werden DXF-Punkte, Feldbuch-, Import- und Neupunkte sowie
   Linienendpunkte, wie beim Messen. Punktnummer und Höhe werden übernommen, sofern vorhanden.
3. Das **Absteckfenster** öffnet sich auf der rechten Hälfte des DXF-Viewers. Die linke Hälfte zeigt
   weiter den Plan mit dem markierten Zielpunkt. Ein anderer Punkt im Plan wechselt das Ziel direkt.
4. Das Fernrohr nach der angezeigten Richtung drehen. Die Fernrohrrichtung läuft live mit.
5. **Messen** (Knopf oder A-Taste/„€“, also Strg+Alt+E). Die Korrekturen erscheinen als Pfeile und Zahlen.
6. Den Reflektor versetzen und neu messen, bis alle Werte innerhalb der Toleranz sind (grün).
7. **„Abgesteckt“** speichert das Ergebnis. Danach den nächsten Punkt antippen oder das Fenster schließen.

## 2. Absteckfenster

```
┌───────────────────────────────────────────┐
│  Ziel 1023   Hz soll 123.4560   s 45.832  │  Kopfzeile (Soll)
├───────────────────────────────────────────┤
│                  ◎ 1023                    │  Zielpunkt oben (Soll)
│              ┌─────────┐                   │
│              │ ✚ Ist    │  ← Zoomkreis um   │
│              │  ↑ 0.35  │    das Ziel        │
│              └─────────┘    (automatisch    │
│                  │          1 m / 10 cm / 1 cm)
│                 ╱│  Fernrohrrichtung (live) │
│                ╱ │                           │
│               ╱  │  Sollrichtung             │
│                  ▲                          │
│            Tachymeter STP1                  │  Standpunkt unten
├───────────────────────────────────────────┤
│ Drehen:  ↻ 0.0123 gon rechts  (quer 9 mm) │  live, vor der Messung
│ Längs:   0.350 m  ZURÜCK                   │  nach der Messung
│ Quer:    0.012 m  nach LINKS               │
│ Höhe:    +0.045 m  Auftrag (bei Sollhöhe)  │
│ ZH [m] [1.700]   Code [___]                │
│ [ Messen (€) ] [ Anfahren ] [ Abgesteckt ] [ Schließen ] │
└───────────────────────────────────────────┘
```

- **Darstellung:** Die Linie Standpunkt → Ziel steht immer senkrecht, mit dem Standpunkt unten und dem Ziel
  oben. Damit entsprechen „längs“ und „quer“ direkt oben/unten und links/rechts im Bild.
- **Fernrohrrichtung:** Ein Strahl vom Standpunkt aus mit der Live-Richtung Hz. Dazu kommen ΔHz in gon und
  die daraus folgende Querablage am Ziel in mm, also s · sin ΔHz. Bei kleinen Winkeln wird der Strahl
  sichtbar überhöht gezeichnet; die Zahlen bleiben exakt.
- **Zoomkreis um das Ziel:** Ringe in automatisch passendem Maßstab. Der Ist-Punkt der letzten Messung
  erscheint als Kreuz. Pfeile zeigen die nötige Bewegung. Die Farbe richtet sich nach der Toleranz:
  grün innerhalb, orange bis zum Doppelten, sonst rot.
- **Rechnung:** Die vorhandenen Funktionen aus `rechnen::absteckung` werden wiederverwendet
  (`polar_zu`, `berechne_einweisung`, `berechne_abweichung`). Grundlage ist die aktive Stationierung
  (Orientierung, Maßstab und Höhe), wie in der Punktabsteckung des Hauptfensters.
- **Höhe:** Hat der Zielpunkt eine Höhe und ist 3D aktiv, gilt ΔH = Soll − Ist aus V, IH und ZH.
  Das Ergebnis wird als Auftrag oder Abtrag angezeigt.

## 3. Speichern und Protokoll

- **Messprotokoll:** Jede Absteckmessung wird zur `MESSUNG` mit dem neuen Typ **`AB`**. Sie enthält die
  Sollkoordinaten in R/H/Höhe und die Abweichungen längs, quer und Höhe im Text. „Abgesteckt“ wird zur
  `BEMERKUNG` mit Bezug auf die letzte Messung.
- **Messdatei (dat.csv):** Die Messung wird als Zeile vom Typ `AB` angehängt. Die Polarpunktberechnung
  überspringt diese Zeilen, wie heute schon die `KP`-Zeilen.
- **Absteckprotokoll:** Das vorhandene Absteckprotokoll (RTF) aus der Punktabsteckung wird beim Schließen
  geschrieben. Es enthält Soll, Ist, Abweichungen und Zeit.
- **Plan:** Abgesteckte Punkte bekommen im Plan ein Häkchen-Symbol.

## 4. Technische Umsetzung (Skizze)

- Neues Modul `gui/dxf_viewer/absteckung.rs` mit `AbsteckSitzung` (Zielpunkt, Sollwerte, letzte Messung,
  Toleranzen, Liste der Abgesteckten) und einer Zeichenfunktion für die Animation.
- Toolbar-Knopf „Abstecken“. Der Moduswechsel wird über die vorhandene Snap- und Punktwahl gelöst
  (`punkt_gewaehlt`).
- Messung über `geraet.starte_messung(zielhoehe)`. Das Ergebnis wird an die Sitzung weitergereicht,
  so wie es heute beim Dialog „Messdaten eingeben“ und bei der Kontrolle geschieht. Die Messtaste
  (`mess_taste`) löst im Abstecken-Modus diese Messung aus.
- Live-Hz über `geraet.live_hz_aktiv`. Das „Anfahren“ nutzt `geraet.starte_anfahren(hz_soll, v)`, nur bei
  Geräten mit Motor.
- **Fensterlage:** siehe Entscheidung A1.

## 5. Zu beachten

- **Punktwahl (audit.md D2):** Die Punktwahl nimmt heute den ersten Marker im Fangradius, nicht den
  nächsten. Beim Abstecken heißt das: im ungünstigen Fall wird der falsche Punkt abgesteckt. Ich empfehle,
  die Punktwahl in diesem Zuge auf „nächster Treffer“ umzustellen. Liegen mehrere Treffer gleich weit weg,
  soll eine Auswahl erscheinen.
- **Getrennte Messung (audit.md K3):** Sie wird beim Abstecken nicht angeboten. Es gibt nur
  Vollmessungen, weil die Winkel sonst veraltet sein könnten.

## 6. Entscheidungen

- **A1 – Fensterart:**
  - (a) **Bereich innerhalb des DXF-Viewer-Fensters** (rechte Hälfte, Plan links). Er folgt Größe und
    Lage des Viewers automatisch, die Messtaste wirkt sicher, und es gibt keine Fokusprobleme auf dem
    Toughpad. *(Vorschlag)*
  - (b) Ein **eigenes Fenster**, das genau auf die rechte Hälfte des Viewers gelegt wird und bei
    Verschieben oder Größenänderung nachgeführt wird. Es sieht gleich aus, kann aber auf dem Toughpad
    Fokus- und Tastaturprobleme haben, etwa wenn die Messtaste im falschen Fenster landet.
- **A2 – Blickrichtung der Anweisungen:**
  - aus Sicht des Instruments, also „zurück, links“ vom Tachymeter aus gesehen *(Vorschlag, üblich)*
  - oder aus Sicht des Reflektorträgers, also „auf mich zu, rechts“
- **A3 – Toleranzen:** Lage und Höhe je 10 mm als Vorgabe, einstellbar im Absteckfenster und gespeichert
  in den Einstellungen.
- **A4 – Höhe mit abstecken**, wenn der Zielpunkt eine Höhe hat: ja *(Vorschlag)* oder nein.
- **A5 – Abgesteckte Ist-Punkte zusätzlich als Neupunkte speichern**, z. B. als `1023A`: ja oder nein.
  Mein Vorschlag ist nein; Messprotokoll, dat.csv und Absteckprotokoll genügen.
- **A6 – „Anfahren“-Knopf** für motorisierte Geräte: ja *(Vorschlag)*. Bei Geräten ohne Motor bleibt er
  ausgeblendet.
- **A7 – Punktwahl (D2) gleich mitkorrigieren:** ja *(Vorschlag)*.
- **Später:** Dauermessung (Tracking) mit laufender Anzeige ohne Messknopf. Sie braucht eine
  verlässliche Zuordnung der Geräteantworten (audit.md K1).

## 7. Entscheidungen und Umsetzung (24.11.2026)

- **A1 = (b):** eigenes Fenster „Absteckung – {Nr}“, immer im Vordergrund, auf der rechten Hälfte des
  DXF-Viewers. Lage und Größe folgen dem Viewer bei jedem Bild. Die Messtaste wirkt in beiden Fenstern.
- **A2:** Anweisungen aus Sicht des Instruments: Längs ZURÜCK/VOR, Quer nach LINKS/RECHTS, Höhe zu hoch
  (Abtrag) bzw. zu tief (Auftrag).
- **A3:** Toleranzen Lage und Höhe in den Rechenparametern (Bereich „Absteckung (DXF-Viewer)“, Vorgabe je
  10 mm).
  - Beim Schließen wird automatisch das Absteckprotokoll (RTF mit Lageplan) geschrieben, sofern
    „Protokollieren“ eingeschaltet ist.
  - Eine offene Messung wird beim Schließen, beim Wechsel des Ziels und bei „Abgesteckt“ abgeschlossen:
    innerhalb der Toleranz automatisch, sonst mit der Rückfrage „Trotzdem übernehmen / Nicht übernehmen /
    Zurück (nachmessen)“.
- **A4:** Die Höhe wird mit abgesteckt, wenn der Punkt eine Höhe hat und die Stationierung 3D ist.
- **A5:** Es werden keine Neupunkte angelegt.
- **A6:** „Anfahren“ erscheint bei GeoCOM-Geräten. Der Zenitwinkel ergibt sich bei 3D aus Soll- und
  Stationshöhe, sonst gilt das aktuelle V.
- **A7:** Die Punktwahl nimmt den nächsten Treffer, unabhängig von der Reihenfolge in der Datei; Marker
  gewinnen nicht mehr automatisch. Bei einer Linie gilt die Nummer des tatsächlich gefangenen Endpunkts.

| Teil | Datei |
|---|---|
| Rechnung (Abweichung längs/quer/Höhe, Querablage des Fernrohrs) | `src/rechnen/absteckung.rs` (`absteck_abweichung`, `querablage_am_ziel`) |
| Absteckfenster, Animation, Ablauf | `src/gui/dxf_viewer/absteckung.rs` |
| Toolbar „Abstecken“, Punktwahl, Messtaste, Messergebnis | `dxf_viewer.rs`, `zeichnen.rs`, `messen.rs`, `messdaten_eingabe.rs` |
| Messtyp `AB` in dat.csv (Polarpunkte überspringen ihn) | `src/daten/messdaten.rs`, `polar_messdaten.rs` |
| Toleranzen | `src/daten/rechenparameter.rs`, `src/gui/rechenparameter_ansicht.rs` |
| Punktwahl (D2) | `DxfViewer::pick`, `naechster_treffer` |

Bekannte Einschränkungen:
- Zentrierte Dialoge des Viewers (z. B. Hinweise) liegen während der Absteckung zur Hälfte unter dem
  Absteckfenster.
- Ohne GeoCOM gibt es kein „Anfahren“.
- Liegen zwei Treffer gleich weit weg, gewinnt der erste. Eine Auswahlliste fehlt noch.
