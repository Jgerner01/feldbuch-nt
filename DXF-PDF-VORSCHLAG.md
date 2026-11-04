# Vorschlag: maßstäblicher Vektor-PDF-Export

## Bedienung im DXF-Viewer

Ein neuer Schalter „PDF-Ausschnitt“ blendet eine A4-Maske über der Zeichnung ein.
Ihre Lage wird in Weltkoordinaten gespeichert. Maßstab und Hoch-/Querformat stammen
aus den Rechenparametern und können vor dem Export dort geändert werden.
Die Maske startet in der Ansichtsmitte und lässt sich am Rahmen verschieben.
Zoomen und Verschieben der Bildschirmansicht verändern den gewählten
Geländeausschnitt nicht. „Maske einpassen“ macht eine zu große Maske vollständig sichtbar.

Die Vorschau zeigt den äußeren Papierrand, den inneren Kartenrahmen und eine
Fußzeile. Außerhalb des Kartenrahmens wird die Bildschirmgrafik abgedunkelt.
„Als PDF speichern …“ exportiert exakt diesen Ausschnitt auf eine A4-Seite.

## Papier und Maßstab

- A4: 210 × 297 mm bzw. 297 × 210 mm.
- Rahmenabstand: links 20 mm, oben und rechts 10 mm. Unterer Seitenrand wie bisher
  (Vorgabe 10 mm), zusätzlich 15 mm Fußzeile innerhalb des unteren Randes.
- Kartenbreite im Gelände: Kartenbreite in mm × Maßstabsnenner / 1000.
- Kartenhöhe entsprechend. Beispiel 1:1000, Hochformat: 180 × 262 m Kartenfläche.
- Ein- und Ausgabe rechnen mit lokalen Differenzen zum Ausschnittszentrum,
  damit große Vermessungskoordinaten ihre Genauigkeit behalten.
- Papierkoordinaten werden erst für die PDF in Punkte umgerechnet (72 Punkte/Zoll).
- Keine automatische Anpassung an den Inhalt: Der festgelegte Maßstab bleibt erhalten.
  Beim späteren Drucken ist „Tatsächliche Größe / 100 %“ erforderlich.

## Gleiche Darstellung auf Bildschirm und Papier

Die bestehende sichtbare Szene wird einmal zusammengestellt und von einem
gemeinsamen Zeichenmodell an zwei Ausgaben übergeben: Bildschirm und PDF.
So gelten identische Ebenensichtbarkeit, Farben, Punktkategorien, Beschriftungen,
Residuen und Fehlerellipsen samt deren Vergrößerungsfaktor.
Die aktuelle Reihenfolge der Zeichenelemente bleibt erhalten.
Auswahlmarkierungen, Fanghilfen, Mauszeiger und A4-Bedienrahmen werden nicht gedruckt.

Linien, Polylinien, Kreise, Bögen, Symbole und Ellipsen werden als PDF-Pfade
geschrieben. Beschriftungen bleiben Text mit eingebetteter Schrift einschließlich
Umlauten. Die PDF wird direkt aus Geometrie erzeugt, ohne Bildschirmfoto oder
Rasterzwischenstufe. Karteninhalt wird am inneren Kartenrahmen abgeschnitten.

Bildschirmgrößen, die bisher in logischen Pixeln vorliegen, erhalten eine gemeinsame
physische Stildefinition. Dabei dienen die bisherigen 96 dpi des Viewers als
Umrechnungsgrundlage. Die Druckvorschau zeigt die tatsächlich wirksamen Papiergrößen:

- Linienbreite = Maximum aus Darstellungsbreite und PDF-Mindestlinienbreite,
  mindestens 0,25 mm.
- Größe von Punktsymbolen = Maximum aus Darstellungsgröße und PDF-Mindestgröße,
  mindestens 2 mm äußere Gesamtgröße (Kreis: Durchmesser, Kreuz: gesamte Spannweite).
- Echte Geometrien wie vermessene Kreise und Fehlerellipsen behalten ihre
  geometrischen Abmessungen; die Symbol-Mindestgröße gilt nur für Punktzeichen.

## Maßstabsleiste

Der Kartenrahmen ist 0,5 mm stark. Die Fußzeile enthält „1 : n“ in 12 Punkt,
eine schwarz-weiße Vektor-Maßstabsleiste mit
Meterbeschriftung sowie optional Projektname und Datum. Ihre Länge wird aus einer
1–2–5-Abstufung gewählt, sodass sie auf dem Papier etwa 40–80 mm breit ist.
Die Leiste verwendet denselben Umrechnungsfaktor wie die Karte. Ein gesonderter,
bereits vorhandener Fehlerellipsen-Maßstab bleibt bei sichtbaren Ellipsen erhalten
und wird eindeutig als Vergrößerung der Fehlerellipsen bezeichnet.

## Rechenparameter

Die Oberfläche ist in farbige Bereiche gegliedert: Fehlergrenzen (blau),
Stationierung/Standardabweichung (grün), Polar-Maßstab (violett),
Gewichtung/Zentrierfehler (ocker), DXF/PDF (türkis).

PDF-Vorgaben: Maßstab 1:1000, A4 Hochformat, unterer Rand 10 mm,
Mindestlinienbreite 0,25 mm und Mindestgröße der Punktsymbole 2 mm.
Sie werden in der vorhandenen Rechenparameter-XML gespeichert. Alte Dateien ohne
PDF-Einträge erhalten diese Vorgaben. Unterschreitungen der Mindestmaße und
nicht endliche Zahlen werden beim Einlesen abgefangen.

## Umsetzung und Prüfung

1. Sichtbare Szene und Zeichenstile aus `zeichnen.rs` und `dxf/entity.rs`
   gemeinsam nutzbar machen; Weltgeometrie von Bildschirmtransformation trennen.
2. A4-Ausschnitt, Rahmenverschiebung und Druckvorschau implementieren.
3. PDF-Ausgabe mit Pfaden, eingebetteter Schrift, Karten-Clipping und Maßstabsleiste
   ergänzen; Einstellungen unmittelbar vor dem Export aus dem Projekt lesen.
4. Hoch-/Querformat und mehrere Maßstäbe anhand bekannter Strecken prüfen.
   PDF strukturell auf Vektoren, Schrift, A4-Maße und Mindestgrößen prüfen sowie
   eine gerenderte Ausgabe mit der Vorschau vergleichen.

Status: Parameteroberfläche, A4-Maske, gemeinsame Szenenauswahl und Vektor-PDF-Ausgabe
sind umgesetzt. Im Viewer oben neben dem Laserpointer den farbigen „PDF“-Schalter aktivieren, Papierformat und Maßstab wählen,
„Maske einpassen“ verwenden, dann den Rahmen zum gewünschten Ausschnitt ziehen und
„PDF speichern …“ wählen. Die Mindestgrößen und der Seitenrand stehen in den
Rechenparametern. Die Fußzeile enthält Projektname, Datum und bei sichtbaren
Fehlerellipsen deren Vergrößerungsfaktor.
Nach erfolgreichem Speichern wird die PDF-Funktion automatisch ausgeschaltet;
bei Abbruch oder Fehler bleibt sie aktiv.

Linien und Polylinien werden als Pfade, Kreise als kubische Kurven und die bereits
im Viewer segmentierten Bögen und Fehlerellipsen als Vektorlinien ausgegeben.
Text bleibt auswählbarer Text mit eingebetteten Schriften. Nicht unterstützte
Zeichenformen oder Schriftzeichen führen zu einer Fehlermeldung statt zu einer
unvollständigen PDF. Während die Maske aktiv ist, dienen Mausaktionen ausschließlich
der Ausschnittwahl und Ansichtsnavigation.
