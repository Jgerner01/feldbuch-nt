# Feldbuch-NT

Feldbuch für Tachymeter-Aufnahmen und Absteckungen (Rust, egui/eframe, eine einzelne EXE).
Stand der Portierung von C# nach Rust: siehe [PORTIERUNG.md](PORTIERUNG.md).

## Panasonic Toughpad FZ-M1: A-Taste als Messtaste

Die **A-Taste** des Panasonic Toughpad löst im DXF-Viewer die Messung aus. Dafür muss die A-Taste
einmalig auf **Strg + Alt + E** gelegt werden. Auf deutscher Tastatur ist das AltGr + E, also das
Zeichen **€**. Feldbuch-NT erkennt diese Tastenkombination als A-Taste und blendet das erzeugte
„€" aus, es landet also in keinem Eingabefeld.

### Einrichtung (einmalig am Gerät)

1. **Panasonic PC Settings Utility** öffnen, dort *Button / Key*, dann *Tablet Buttons*.
2. Die **A-Taste** wählen und bei *Press* die Tastenkombination **Strg + Alt + E** einstellen.
   (Eine Belegung mit F13 ist im Dialog nicht auswählbar.)
3. Feldbuch-NT starten, im DXF-Viewer in den Modus *Neupunkte* wechseln.

Alternativ lässt sich die Belegung in der Systemsteuerung unter *Hardware und Sound*,
„Tablet-Tasten für bestimmte Aufgaben festlegen" vornehmen oder im Setup Utility (BIOS) unter
*Main*, *Tablet Buttons Configuration*.

### Wirkung der A-Taste

| Situation | Wirkung |
|---|---|
| DXF-Viewer, Modus Neupunkte, kein Dialog offen | wie der Button **► Messen** |
| Getrennte Messung, Strecke bereits gemessen | wie **Übernehmen** (Hz und V vom Gerät) |
| Dialog „Messdaten eingeben" offen | wie **► Messen** im Dialog |
| Modus Stationierung | keine Wirkung (dort wird der identische Punkt im Plan gewählt) |

### Hinweise

- Auf dem Windows-Anmeldebildschirm gelten immer die Standardfunktionen der Tablet-Tasten.
- Ein normal mit AltGr + E getipptes „€" löst im DXF-Viewer ebenfalls die Messung aus und wird nicht
  in Felder eingefügt.
- Die Taste ist im Code festgelegt (`MESS_TASTE` und `MESS_TASTE_ZEICHEN` in
  `src/gui/dxf_viewer.rs`).

## Weitere Toughpad-Funktionen

- **Bildschirmtastatur:** Sobald in einem Eingabefeld der Cursor steht, erscheint am Bildschirmrand eine
  Tastatur (in jedem Fenster, auch in Tabellen). Textfelder (Standpunkt, Projektname, Verzeichnis,
  Aufstellung, Gerätebefehl) bekommen die volle QWERTZ-Tastatur, alle anderen Felder einen Ziffernblock
  (mit ⌫, Tab und OK). Beim Antippen eines Feldes wird dessen Inhalt markiert, die erste Eingabe ersetzt
  ihn. Die Tastatur liegt oben, wenn das Feld in der unteren Bildschirmhälfte steht, sonst unten. Sie
  erscheint nur auf Panasonic Toughpad und Toughbook (Erkennung über die BIOS-Angaben, Hersteller
  Panasonic und Modell `FZ-` oder `CF-`). Weitere Textfelder werden im Code mit
  `bildschirmtastatur::text_feld(&antwort)` gekennzeichnet.
- **Töne:** Nach einer erfolgreichen Messung erklingt ein Bestätigungston (400 Hz und 600 Hz,
  zusammen 0,5 s), nach einer fehlgeschlagenen Messung ein Fehlerton (dreimal 800 Hz, zusammen 0,5 s).
