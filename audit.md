# Feldbuch-NT – Arbeitsplan für die Hochbefunde

Erstellt: **23. November 2026**  
Grundlage: Analysebericht vom 22. November 2026, **Version 0.8.0, Build 50, Quellstand `b5a48fc`**.  
Status (23.11.2026): **S1, S3, D1, P3 erledigt (Abschnitt „Umsetzungsstand“); K2, K3, S2, P1, P2, P4 vorerst zurückgestellt; K1, Q1, D2, D3 offen.**

Dieses Dokument überführt ausschließlich die im Bericht als **Hoch** eingestuften Probleme in umsetzbare Aufgaben. **D3** ist eingeschlossen, weil es bei Plänen mit den betroffenen Geometrieeigenschaften als Hoch eingestuft wurde. Die IDs entsprechen dem Analysebericht.

Dateipfade sind relativ zum Repository. Zeilennummern beziehen sich auf den geprüften Stand und können nach Änderungen abweichen. Vor Beginn eines Pakets den aktuellen Aufrufpfad prüfen und klären, ob zwischenzeitlich bereits eine Teilkorrektur erfolgt ist. Einen Befund erst nach Erfüllung seiner Abschlusskriterien schließen.

## Bearbeitungsübersicht

| Status | ID | Arbeitspaket | Abhängigkeit beziehungsweise Zusammenhang |
|---|---|---|---|
| [ ] | K1 | Geräteantworten eindeutig einem tatsächlich gesendeten Auftrag zuordnen | Grundlage für K2 und K3 |
| zurückgestellt | K2 | Gewünschte und bestätigte Prismenkonfiguration abgleichen | K1 für sichere Quittierungs- und Rücklesefolgen |
| zurückgestellt | K3 | Getrennte Messungen nur mit frischen, passenden Winkeln übernehmen | K1; Verbindungssession und Messkontext |
| [x] | S1 | Deaktivierte Beobachtungen aus allen Ergebnisparametern ausschließen | Gemeinsam mit S2 bearbeiten |
| zurückgestellt | S2 | Maßstab und Lageausgleichung auf ein konsistentes Modell stellen | Fachliche Modellentscheidung dokumentieren |
| [x] | S3 | IH-Korrektur mit Stationshöhe und Neupunkten konsistent durchführen | S1/S2 für die Neuberechnung; Q1 für Speicherung |
| zurückgestellt | P1 | Anschlussmessungen nach vollständiger Aufstellung trennen | Gemeinsamer Aufstellungsschlüssel, auch für D1 |
| zurückgestellt | P2 | Gespeicherte Aktivierungen und Fixierungen erhalten | P1; konsistente Berechnungsdefinition |
| [x] | P3 | Exzentrizitäten anhand stabiler Messidentität zurückschreiben | Q1 für einen konsistenten Gesamtvorgang |
| zurückgestellt | P4 | Fehlende Grundlagen vom ausdrücklich lokalen Modus trennen | P1/P2; Koordinatenbezug im Ergebnis |
| [x] | D1 | Räumliche Löschung auf exakt ausgewählte Aufstellungen begrenzen | P1-Schlüssel; Q1 für Erfolg und Protokoll |
| [ ] | D2 | Nächsten Punkt und passende Endpunktnummer bestimmen | Einheitliches Picking-Ergebnis |
| [ ] | D3 | Unterstützte DXF-Geometrie korrekt darstellen oder ausdrücklich ablehnen | Importdiagnose und Picking-Vertrag |
| [ ] | Q1 | Zusammengehörige Speicheränderungen konsistent abschließen | Querschnitt, frühzeitig entwerfen |

**Empfohlene Reihenfolge:** Zuerst die gemeinsamen Verträge für Aufträge, Aufstellungsschlüssel und Revisionen festlegen. Dann K1 → K2/K3, S1/S2, P1/P2, P3/P4 und D1/D2 bearbeiten. S3 und alle schreibenden Abläufe an Q1 anschließen. D3 lässt sich parallel bearbeiten. Q1 als Architekturgrundlage früh beginnen und abschließend an allen betroffenen Abläufen nachweisen.

## Regeln für jedes Arbeitspaket

1. **Fehler vor der Änderung reproduzieren.** Aus den Audit-Prüfständen einen regulären Regressionstest mit richtigem Sollverhalten ableiten. Er muss vor der Reparatur am beschriebenen Fehler scheitern.
2. **Den Produktionspfad reparieren.** Eine vorhandene Hilfsfunktion oder ein bestandener Parser-Einzeltest genügt nicht, wenn die Oberfläche diesen Weg nicht verwendet.
3. **Bestehende Schutzmechanismen erhalten.** Schreibsperren, Backup, Einzeldateiersetzung, Fehlerweitergabe und Rücknahme im Speicher nicht umgehen. Historische Rohdaten nicht stillschweigend ersetzen oder mit neuen IDs versehen.
4. **Fehlerzustände mitprüfen.** Abbruch, Timeout, Neuverbindung, ungültige Zuordnung und Schreibfehler gehören zum jeweiligen Ablauf.
5. **Abschluss belegen.** Geänderte Dateien, Testnamen, Testergebnis, erforderliche Geräteversuche und verbleibende Einschränkungen unter dem Paket eintragen. Erst danach den Status auf erledigt setzen.

Die Baseline bestand **403 vorhandene Tests**. Das ist eine Ausgangsbasis, kein Nachweis für die hier beschriebenen Reparaturen. Die zusätzlichen Audit-Prüfstände sind teilweise absichtlich auf den fehlerhaften Istzustand formuliert: Ihre erfolgreichen Assertions müssen vor einer Übernahme in die Testsuite auf korrektes Sollverhalten umgestellt werden.

Die Anwendung ist derzeit ein Binärprojekt. Tests mit privaten GUI- oder Projektfunktionen möglichst als passende `#[cfg(test)]`-Module oder über bewusst herausgelöste fachliche Schnittstellen integrieren; keine produktiven Funktionen nur für Tests unnötig öffentlich machen.

---

## K1 – Fremde Geräteantworten dürfen keinen Auftrag weiterschalten

**Priorität:** Hoch. **Nachweis:** Im Modulprüfstand ausgeführt.

**Fundstellen:**

- `src/tachymeter/messablauf.rs:194–225`
- `src/tachymeter/schalter.rs:106–131`
- `src/tachymeter/geocom.rs:59–64`
- `src/gui/geraet.rs:419–461`
- Mitprüfen: manuelle Befehle in `src/gui/kommunikation.rs` und `src/gui/geraet.rs`, Signaltöne in `src/tachymeter/hilfen.rs`.

**Fehler und Folge:** Antworten werden anhand des gerade erwarteten Zustands interpretiert. Eine fremde erfolgreiche Antwort kann einen Mess- oder Schaltvorgang quittieren. Mehrere gepufferte Zeilen können bereits Folgezustände durchlaufen, deren Befehl noch gar nicht gesendet wurde. Im Prüfstand wurde so eine fremde Strecke von 123 m als Messergebnis übernommen.

### Lösungsschritte

1. Den vollständigen Weg `Auftrag → Sendequeue → Portthread → tatsächliches Schreiben → Empfang → Zustandsmaschine` dokumentieren. „In Queue aufgenommen“ ausdrücklich von „am Port geschrieben“ unterscheiden.
2. Einen gemeinsamen Anfragenkoordinator für reguläre Messungen, Liveabfragen, Konfigurationsänderungen, Schalter, Testbefehle und Signaltöne einführen. Alle Antwortkonsumenten müssen seine Zuordnung verwenden; keine konkurrierende Quittierungslogik daneben belassen.
3. Jeden Auftrag mit Verbindungssession, interner Anfragekennung, erwartetem Antwortformat, zuständigem Ablauf und Timeout versehen. Vor dem Schreiben darf noch kein Antwort-Wartezustand aktiv sein. Der Portthread muss einen erfolgreichen Schreibabschluss beziehungsweise Schreibfehler zurückmelden.
4. Für GeoCOM die vom jeweiligen Geräteprofil nachweislich unterstützte Transaktionszuordnung verwenden. Eine interne Anfragekennung allein wird durch eine Geräteantwort noch nicht bestätigt. Rückmeldungen sowohl auf Zuordnung als auch auf Struktur und Returncode prüfen.
5. Für Profile ohne verlässliche Transaktionskennung eine eigene, dokumentierte Strategie umsetzen: genau eine passende offene Anfrage, passende Antwortstruktur, geregelter Umgang mit Restantworten. Nach einem Timeout eine nicht eindeutig zuordenbare Antwort nicht als Antwort des nächsten Auftrags akzeptieren. Im Zweifelsfall eine Resynchronisierung oder Neuverbindung verlangen.
6. Bei mehreren Zeilen in einem Empfangsblock darf eine Antwort erst zu einem neuen Schritt gehören, wenn dessen Auftrag tatsächlich gesendet wurde. Unerwartete Antworten im Kommunikationslog mit Ablehnungsgrund erhalten.
7. Bei Abbruch, Timeout und Trennung sämtliche offenen Aufträge abschließen beziehungsweise entwerten. Ungesendete Befehle des Vorgangs aus der Sendequeue entfernen. Bereits gesendete Befehle bis zum Verwerfen ihrer Antwort beziehungsweise bis zur kontrollierten Beendigung der Session als abgebrochen oder ungeklärt führen. Ein Anwendungsabbruch ist noch kein Geräteabbruch. Keine pauschale automatische Wiederholung von Befehlen, deren Wirkung bereits eingetreten sein könnte; Wiederholbarkeit pro Befehl festlegen.

### Regression und Abnahme

- Fremde Winkelantwort quittiert weder Laser noch Messauslösung.
- Alte Vollmessung nach Timeout wird nicht zur nächsten Messung.
- Zwei fremde Antworten in einem Empfangsblock überspringen keinen noch ungesendeten Schritt.
- Doppelte, fragmentierte und vertauschte Antworten verändern keinen anderen Auftrag.
- Eine alte Verbindungssession kann nach Neuverbindung nichts mehr abschließen.
- Schreibfehler startet keinen Antworttimeout für einen vermeintlich gesendeten Befehl.
- Abbruch vor dem tatsächlichen Versand verhindert, dass der alte Vorgang später noch gesendet wird. Manuelle Testbefehle und Signaltöne stören keine laufende Messfolge.
- Normaler Messablauf und Schalter funktionieren weiter für jedes freigegebene Protokoll.

**Erledigt, wenn:** Kein Zustandswechsel ohne passenden, tatsächlich gesendeten Auftrag möglich ist; alle genannten Produktionspfade integriert sind und die Negativtests bestehen. Die endgültige Gerätefreigabe zusätzlich mit echten Antworten und künstlich verursachtem Verbindungsabbruch prüfen.

**Abschlussnachweis:** _Commit, Testnamen, Geräteprofil und Ergebnis ergänzen._

## K2 – Prismenkonfiguration muss vor der Messung wirksam bestätigt sein

**Priorität:** Hoch. **Nachweis:** Produktionspfad im Code belegt; zusätzlicher Geräteversuch erforderlich.

**Fundstellen:** `src/gui/dxf_viewer/messen.rs:92–139, 241–245, 271–273`, `src/gui/geraet.rs:232–240, 583–607`, `src/tachymeter/konfiguration.rs:128–149, 205–285`.

**Fehler und Folge:** Der Prismadialog speichert Wunschwerte, stellt sie aber nicht am Instrument ein. Die spätere Prüfung erfolgt erst nach der Messverarbeitung und kann bei fehlenden Rücklesewerten ausfallen. Eine Messserie kann dadurch mit einer anderen Prismenkonstante als der angezeigten aufgenommen werden.

### Lösungsschritte

1. Die gesamte Messkonfiguration als zusammengehörigen Wert behandeln: Zieltyp, EDM-Modus, Prisma, Konstante mit dokumentierter Gerätekonvention und Verbindungssession.
2. Zustände für `gewünscht`, `Abgleich läuft`, `bestätigt`, `abweichend`, `unbekannt` und bei Bedarf `manuell bestätigt` vorsehen. Ein nur gespeicherter Wunschwert darf nicht als bestätigter Gerätewert dargestellt werden.
3. Den vorhandenen `GeraetAbgleich` fachlich wiederverwenden, aber seine Aufträge in K1 integrieren. Eine blockierende Rücklesefolge nicht im GUI-Thread ausführen. Konfigurationsänderungen und Messauslösung dürfen sich nicht überholen.
4. Eine Änderung im Prismadialog muss den bisherigen Abgleich entwerten und das Setzen samt Rücklesen auslösen. Bei Messbeginn bestätigen, dass genau die aktuelle Konfigurationsrevision in genau dieser Session gültig ist. Eine dokumentierte Rücklese- beziehungsweise Prüfstrategie muss auch Änderungen direkt am Instrument erkennen.
5. Bei abweichender oder fehlender Rückmeldung die Messung mit einer verständlichen Begründung blockieren oder einen ausdrücklich vorgesehenen manuellen Bestätigungsweg anbieten. Ein Profil ohne Rücklesefähigkeit nicht automatisch als abgeglichen behandeln.
6. Die verwendete bestätigte Konfiguration mit der Messung erfassen. Falls ein nachträglich gelesener Zustand widerspricht, betroffene Ergebnisse markieren; keine uneingeschränkte Erfolgsaussage trotz ungeklärter Gerätekorrektur.
7. Keine weitere Software-Prismenkorrektur hinzufügen, solange die vom Instrument bereits angewandte Korrektur und deren Vorzeichenkonvention nicht eindeutig geklärt sind.

### Regression und Abnahme

- Wechsel von Standardprisma auf ein anderes Prisma: Messung startet erst nach passender Bestätigung.
- Falscher Returncode, fehlendes Rücklesen und widersprüchlicher Rücklesewert führen zu einem ungeklärten beziehungsweise fehlerhaften Zustand.
- Schnelle aufeinanderfolgende Änderungen bestätigen nur die zuletzt gewünschte Revision.
- Schlägt einer von mehreren Setzbefehlen fehl, bleibt die Gesamtkonfiguration unbestätigt. Eine Änderung direkt am Instrument darf nicht dauerhaft mit einem alten Bestätigungsstatus überdeckt werden.
- Neuverbindung entwertet eine Bestätigung aus der alten Session.
- Profile ohne automatischen Abgleich zeigen den dokumentierten manuellen Zustand.
- Am realen Gerät angezeigte und tatsächlich gesetzte Konstante einschließlich Einheit und Vorzeichen vergleichen; mindestens einen bekannten Streckenunterschied kontrollieren.

**Erledigt, wenn:** Anzeige, Messfreigabe und Protokoll den nachweislich verwendeten Zustand wiedergeben. Ein grüner GUI-Status allein genügt nicht als Geräteabnahme.

**Abschlussnachweis:** _Commit, Simulationstests und Geräteprotokoll ergänzen._

## K3 – Getrennte Messung benötigt eine frische Winkelübernahme

**Priorität:** Hoch. **Nachweis:** Codepfad belegt.

**Fundstellen:** `src/gui/geraet.rs:79–91, 629–633`, `src/gui/dxf_viewer/messen.rs:199–210`, `src/gui/dxf_viewer/messdaten_eingabe.rs:299–310`.

**Fehler und Folge:** Nach ausbleibenden Liveantworten bleiben Hz und V als vorhandene Werte stehen. „Übernehmen“ verbindet diese ohne Alters- oder Sessionprüfung mit der zuvor gemessenen Strecke.

### Lösungsschritte

1. Ein gemeinsames Modell für Winkelproben einführen: Hz, V, Empfangszeit auf monotoner Uhr, Session, Anfragekennung und Gültigkeitsstatus. Zusätzlich den protokollierbaren Zeitstempel erhalten. Soweit das Instrument einen fachlich verstandenen Messzeitbezug liefert, diesen mitführen: Ein frischer Empfangszeitpunkt beweist allein keine frische Erfassung.
2. Timeout und Trennung müssen die Verwendbarkeit der Probe entwerten. Für eine reine Anzeige dürfen alte Werte sichtbar bleiben, müssen dann aber ausdrücklich als veraltet markiert sein.
3. Beim Start der getrennten Messung ihren Kontext fixieren: Projekt, AufstellungId, Punkt, Streckenmessung, Zielhöhe und Konfiguration. Änderungen des Kontexts dürfen keinen noch offenen Auftrag umwidmen.
4. Beim Übernahmewunsch über K1 eine neue passende Hz/V-Abfrage auslösen und erst mit deren gültiger Antwort abschließen. Nicht lediglich prüfen, ob irgendwann nach der Streckenmessung ein Livewert empfangen wurde.
5. Mehrfachklicks auf Übernehmen zusammenführen beziehungsweise sperren. Abbruch und Neuverbindung verwerfen die ausstehende Übernahme.
6. Denselben Mechanismus im Viewer und im Dialog „Messdaten eingeben“ verwenden. Strecke und spätere Winkelprobe mit ihren jeweiligen Zeitpunkten und ihrer Herkunft dokumentieren. Die fachliche Bedeutung der gehaltenen Strecke erhalten; Schrägstrecke, Horizontalstrecke und Höhenmodell nicht beiläufig ändern. Einen zusätzlichen Altersgrenzwert nur nach fachlicher Festlegung verwenden.

### Regression und Abnahme

- Gültige Winkel empfangen, danach Liveantworten unterdrücken, neu ausrichten und Übernehmen drücken: kein Punkt aus den Altwerten.
- Eine Winkelantwort vor dem Übernahmewunsch erfüllt den neuen Auftrag nicht.
- Verzögerte Antwort der falschen Anfrage oder Session wird abgelehnt.
- Wechsel der Aufstellung oder des Messkontexts führt nicht zur Speicherung unter dem neuen Ziel.
- Erfolgreiche getrennte Messung ergibt genau einen Datensatz mit nachvollziehbarer Zeitzuordnung.

**Erledigt, wenn:** Beide Bedienpfade ausschließlich eine frische, zugeordnete Probe übernehmen und veraltete Werte weder Messung noch Erfolgssignal auslösen.

**Abschlussnachweis:** _Commit, Testnamen und Geräteversuch ergänzen._

---

## S1 – Deaktivierte Beobachtungen dürfen den Maßstab nicht beeinflussen

**Priorität:** Hoch. **Nachweis:** Am unveränderten Rechenkern numerisch reproduziert.

**Fundstellen:** `src/rechnen/freie_stationierung.rs:136, 345–378`; Verwendung des Maßstabs in `src/rechnen/polar.rs`.

**Fehlerbild:** Ein vollständig deaktiviertes viertes Ziel wird um 0,100 m verändert. Lage und s₀ bleiben unverändert, der Maßstab wechselt von 1 auf etwa 0,999863438. Das wirkt sich im Modus „berechnet“ bei 100 m Polarstrecke um rund 13,656 mm aus.

### Lösungsschritte

1. Die Wirksamkeit der Aktivierungslisten für Richtung, Strecke und Höhe an jeder Rechenstufe explizit festlegen: Prüfung, Näherung, Maßstab, Iteration, Residuen und Qualitätsausgabe.
2. Die Helmert-Initialisierung darf nicht mehr ungeprüft alle Zeilen erhalten. Ihre Schnittstelle um wirksame Beobachtungen erweitern beziehungsweise eine geeignete aktive Teilmenge mit stabiler Rückzuordnung auf die Originalzeilen bilden.
3. Eine einzelne deaktivierte Komponente nicht als Bestandteil einer künstlichen vollständigen Polarposition verwenden. Aus aktiver Richtung und ausgeschalteter Strecke lässt sich keine beobachtete kartesische Position mit der ausgeschalteten Strecke begründen.
4. Wenn die aktiven Beobachtungen keine Initialisierung oder Maßstabsbestimmung tragen, eine nachvollziehbare eingeschränkte Lösung oder einen Fehler liefern. Nicht heimlich abgeschaltete Werte als Ersatz aktivieren.
5. Maßstab und etwaige Ersatzschätzungen ausschließlich aus den nach S2 definierten wirksamen Beobachtungen ermitteln. Inaktive Zeilen dürfen zu Vergleichszwecken angezeigt werden, aber keinen ausgegebenen Ergebnisparameter bestimmen.
6. Die Abbildung zwischen Messzeile und Residuum erhalten. Wiederholte Punktnummern dürfen dabei nicht versehentlich zusammenfallen.

### Regression und Abnahme

- Vorhandenen Maßstabsprüffall übernehmen und auf **unveränderten Maßstab bei Änderung des deaktivierten Ziels** umstellen.
- Hz, V, Strecke, Koordinaten und Zielhöhe vollständig deaktivierter Ziele einzeln sowie gemeinsam verändern: sämtliche nutzbaren Ergebnisparameter bleiben gleich.
- Teilaktivierte Ziele prüfen: Eine ausgeschaltete Strecke beeinflusst die Lösung nicht; die noch aktive Richtung darf sie beeinflussen.
- Mehrere unterschiedliche Näherungen und ungünstige, aber gültige aktive Geometrien prüfen. Fachlich nicht bestimmbare Fälle ausdrücklich ablehnen.
- Vergleich in geeigneten numerischen Toleranzen dokumentieren; keine Rundung der Ausgabe zur Kaschierung von Änderungen verwenden.

**Erledigt, wenn:** Die Aktivierung im gesamten Rechenweg gilt und die Metamorphietests auch den für Polarpunkte verwendeten Maßstab absichern.

**Abschlussnachweis:** _Commit, Testfälle, Vergleichstoleranzen ergänzen._

## S2 – Maßstab und Lageausgleichung müssen dasselbe Beobachtungsmodell nutzen

**Priorität:** Hoch. **Nachweis:** Am unveränderten Rechenkern numerisch reproduziert.

**Fundstellen:** `src/rechnen/freie_stationierung.rs:136, 345–378, 429–437, 475`.

**Fehlerbild:** Vier fehlerfreie Beobachtungen mit wahrem Maßstab 1,0001 ergeben zwar den erwarteten Maßstab, aber etwa 2,626 mm Lagefehler und s₀≈4,447 mm. Der aus der Näherung stammende Maßstab geht nicht konsistent in die Streckengleichungen ein.

### Lösungsschritte

1. Vor der Implementierung die Bedeutung von `massstab` festschreiben: In welcher Richtung wird korrigiert, und gilt die Gleichung `Koordinatenstrecke = m × gemessene Horizontalstrecke`? Alle Aufrufer einschließlich Polarberechnung und Kontrollmessung daran ausrichten.
2. Zwei Fälle getrennt modellieren: **fachlich festgelegter Maßstab** und **aus Beobachtungen zu schätzender Maßstab**. Ein aus denselben Daten geschätzter Näherungsmaßstab ist nicht ohne Weiteres ein unabhängiger fehlerfreier Festwert.
3. Für den Modus „freier Maßstab“ vorzugsweise Lage, Orientierung und Maßstab gemeinsam ausgleichen. Den Parametervektor, die zugehörige Ableitung in der Streckengleichung und die Normalgleichungen konsistent erweitern. Falls stattdessen ein fester Maßstab vorgesehen wird, muss dies als eigenständiger fachlicher Modus dokumentiert werden. Wird ein zuvor geschätzter Faktor anschließend festgehalten, die Qualitätskennwerte ausdrücklich als bedingt auf diesen Faktor kennzeichnen; seine Schätzunsicherheit nicht als vollständig berücksichtigt darstellen.
4. Maßstab in jeder Iteration, bei Endresiduen und beim Soll-Ist-Vergleich nach demselben Modell verwenden. Gewichte müssen zu den verwendeten Beobachtungen und deren Einheiten passen.
5. Bestimmbarkeit, Rangprüfung, Unbekanntenanzahl, Redundanz, s₀ und Kovarianz beziehungsweise Konfidenzellipse aktualisieren. Zweipunktfall sowie gemischte Richtungs-/Streckenaktivierung ausdrücklich berücksichtigen.
6. Alle Verwendungen des Ergebnismaßstabs suchen. Eine Strecke darf nicht in der Stationierung und anschließend in einer weiteren Stufe unbeabsichtigt doppelt korrigiert werden. Exzentrizitäten bleiben vom Streckenmaßstab getrennt.
7. S1 in derselben Änderung absichern oder unmittelbar davor korrigieren, damit die neue Modellierung keine deaktivierten Daten übernimmt. Das verwendete Modell mit einer Berechnungsrevision in Stationsergebnis und Protokoll speichern; ältere Ergebnisse beim Laden nicht ungeprüft mit dem neuen Modell vermischen.

### Regression und Abnahme

- Synthetische Referenzen mit `m=1`, `m=1,0001`, `m=1,001` und einem plausiblen Wert unter 1 erzeugen. Station, Orientierung und Maßstab müssen bei fehlerfreien Daten numerisch zur Referenz passen.
- Freier und fester Maßstab liefern jeweils die für ihren Modus erwarteten Residuen und Freiheitsgrade.
- Zweipunktfall, zusätzliche Ziele, Wiederholungen, teilaktive Beobachtungen und rangdefiziente Geometrie prüfen.
- Eine unabhängige Referenzrechnung oder explizite Herleitung zur Kontrolle von Vorzeichen, Ableitungen und Einheiten verwenden. Tests dürfen nicht nur die gleiche Implementierung nachbilden.
- End-to-End-Test: Stationierung und anschließende Polarpunktberechnung rekonstruieren bekannte Koordinaten ohne doppelte Maßstabsanwendung.

**Erledigt, wenn:** Rechenmodell, Ausgabe, Qualitätskennwerte und alle relevanten Aufrufer fachlich zusammenpassen. Die Modellentscheidung und die erwarteten numerischen Toleranzen sind dokumentiert.

**Abschlussnachweis:** _Modellentscheidung, Commit, Referenzrechnung, Testnamen ergänzen._

## S3 – Instrumentenhöhe unter Erhaltung der vollständigen Höhenkette korrigieren

**Priorität:** Hoch. **Nachweis:** Mit Originalfunktionen der Arbeitskopie ausgeführt.

**Fundstellen:** `src/daten/projekt.rs:449–474`, `src/rechnen/freie_stationierung.rs:192`, `src/rechnen/neupunkt.rs:68–73`, `src/gui/dxf_viewer/standpunkt.rs:243`.

**Fehlerbild:** Eine IH-Korrektur von 1,500 auf 1,650 m lässt die frei bestimmte Stationshöhe unverändert und verschiebt Neupunkte um +150 mm. Im Prüfaufbau müsste die Stationshöhe um −150 mm korrigiert werden; die aus denselben Aufnahmen abgeleiteten Neupunkthöhen bleiben dann gleich.

### Lösungsschritte

1. Eine **Korrektur einer bestehenden Aufstellung** von einer **tatsächlichen Änderung des Instrumentaufbaus** unterscheiden. Letztere darf nicht als rückwirkende IH-Korrektur für bereits gemessene Punkte behandelt werden.
2. Vor dem Ändern den vollständigen bisherigen Zustand erfassen: AufstellungId, IH, freie oder bekannte Station, aktive Beobachtungen, Berechnungsparameter, Ergebnis und davon abhängige Punkte.
3. Bei frei bestimmter Stationshöhe die Station aus den Anschlussmessungen mit der korrigierten IH und denselben wirksamen Randbedingungen neu berechnen. Nicht nur IH an eine unveränderte Stationshöhe anhängen.
4. Bei bekanntem festen Standpunkt dessen feste Bodenhöhe erhalten und die Höhenbeobachtungen kontrollieren. Eine korrekte Änderung der Neupunkthöhen kann hier fachlich erforderlich sein; nicht pauschal die Kompensation aus dem freien Fall anwenden. Ist nur die Lage bekannt und wird die Höhe aus Beobachtungen abgeleitet, gilt für die Höhe weiterhin der abgeleitete Fall.
5. Erst mit dem neuen gültigen Stationsergebnis alle zugehörigen Neupunkte neu berechnen. 2D-Ergebnisse, fehlende Höhen und lokal definierte Höhenbezüge getrennt behandeln.
6. Änderung von Station, IH, Punkten und Protokoll nach Q1 als einen konsistenten Vorgang abschließen. Bei fehlenden Grundlagen oder Schreibfehlern keinen vollständigen Erfolg melden.
7. In der Bestätigung die Wirkung nennen: welche Aufstellung und Punkte betroffen sind, ob die Stationshöhe neu bestimmt wird und welche Höhen dadurch geändert werden.

### Regression und Abnahme

- Den vorhandenen IH-Prüffall auf das richtige Soll ändern: alte Neupunkthöhe 98,500 m bleibt bei einer reinen IH-Korrektur der freien Aufstellung 98,500 m; die Stationshöhe wechselt dabei von 98,500 auf 98,350 m.
- Bekannter Standpunkt mit fester Höhe: erwartete Neupunkthöhen unabhängig herleiten und prüfen.
- 2D, fehlende Höhenbeobachtungen, ungültige Neuberechnung, weitere Aufstellung derselben Nummer und Speicherung mit Dateisperre testen.
- Wiederholte Korrekturen und Rückkorrektur dürfen keine kumulierten Verschiebungen erzeugen.

**Erledigt, wenn:** Freie und bekannte Station unterschiedliche, fachlich korrekte Höhenketten verwenden und sämtliche betroffenen Punkte zur gespeicherten Stationsrevision passen.

**Abschlussnachweis:** _Commit, freie/bekannte Referenzfälle, Fehlerpfadtests ergänzen._

---

## P1 – Anschlussmessungen nach vollständiger Aufstellung auswählen

**Priorität:** Hoch. **Nachweis:** Mit Originalfunktionen der Arbeitskopie ausgeführt.

**Fundstellen:** `src/gui/polarpunkte.rs:306–313, 425–432, 465–470`.

**Fehlerbild:** Die Neupunktgruppe verwendet AufstellungId und Standpunkt, die Anschlussquelle `dat.csv` filtert nur die Standpunktnummer. Die Auswahl `S1 [alt]` lud im Prüfstand vier statt zwei Anschlüsse, weil die neue Aufstellung ebenfalls `S1` hieß.

### Lösungsschritte

1. Einen typisierten Schlüssel für die ausgewählte Aufstellung schaffen. Anzeigenamen wie `S1 [alt]` nicht als fachlichen Schlüssel parsen. Die stabile AufstellungId und der zugehörige Standpunkt müssen getrennt verfügbar bleiben.
2. Dieselbe zentrale Zuordnungsfunktion für Anschlusspunkte, Neupunkte, IH, gespeicherte Station und gewählte `stp.csv` verwenden. Alle bisherigen Filter nur nach `standpunkt_nr` im Polarmodul prüfen.
3. Eine bekannte AufstellungId muss exakt übereinstimmen. Eine abweichende ID darf nicht allein wegen gleicher Standpunktnummer als passend gelten.
4. Für Altdaten ohne ID eine begrenzte, dokumentierte Migrationsregel festlegen. Bei mehreren möglichen Aufstellungen eine Zuordnung verlangen und bis dahin die Berechnung sperren. Keine historische Messung ohne Bestätigung der aktuell aktiven Aufstellung zuordnen.
5. Beim Gruppenwechsel Anschlussliste, Aktivierungen, Ergebnis, Vorschau und etwaige ausstehende Speicherbestätigung vollständig aktualisieren. Keine Elemente der zuvor gewählten Gruppe übernehmen.
6. Gewählte Daten aus externen DAT-/STP-Dateien mit ihren Metadaten prüfen. Gleiche Namen in zwei Projekten oder in zwei Dateien begründen keine identische Aufstellung.

### Regression und Abnahme

- Vorhandenen Vier-AP-Prüffall auf genau zwei Anschlüsse der gewählten Aufstellung umstellen.
- Gleiche Standpunktnummer, gleiche Zielnummern, aber andere Orientierung oder IH in einer zweiten Aufstellung: jedes Ergebnis bleibt unabhängig.
- Umbenannte Standpunktnummer bei stabiler ID sowie mehrdeutige Altdateien ohne ID separat prüfen.
- Die jeweils andere Aufstellung verändern: Ergebnis der gewählten Gruppe bleibt unverändert.

**Erledigt, wenn:** Sämtliche Berechnungsgrundlagen einer Polargruppe eindeutig derselben bestätigten Aufstellung zugeordnet sind.

**Abschlussnachweis:** _Commit, Zuordnungsregel für Altdateien, Tests ergänzen._

## P2 – Gespeicherte Berechnungsbedingungen vollständig übernehmen

**Priorität:** Hoch. **Nachweis:** Reaktivierung gespeicherter Haken ausgeführt; Verlust der Fixierung eines bekannten Standpunkts im Codepfad belegt.

**Fundstellen:** `src/gui/polarpunkte.rs:431, 440–443, 515–526`, Stationsmodell in `src/daten/projekt.rs:61–97`.

**Fehlerbild:** Die Quelle „gespeicherte Stationierung“ übernimmt nur Messzeilen und aktiviert sie anschließend neu. Gespeicherte Richtungsflags `[true,false]` wurden zu vollständig aktiven Zeilen. Der bekannte feste Standpunkt wird in diesem Ladepfad ebenfalls nicht als solche Randbedingung übernommen.

### Lösungsschritte

1. Den vollständigen Stationsdatensatz nach P1 laden: ID, Messungen, Aktivierung, IH, bekannte/freie Lage- und Höhenbedingungen sowie relevante Parameter beziehungsweise Parameterrevision.
2. Die Bedienbedeutung festlegen: **gespeichertes Ergebnis verwenden** und **aus gespeicherten Beobachtungen neu rechnen** als unterscheidbare Aktionen anbieten. Ein gespeichertes Ergebnis nicht unter geänderten aktuellen Parametern unbemerkt ersetzen.
3. Beim Nachrechnen gespeicherte Richtungs-, Strecken- und Höhenaktivierungen anwenden. Zuordnung nach stabiler Beobachtungsidentität; bei Legacy-Daten mindestens Anzahl, Reihenfolge und Punktzuordnung vollständig prüfen. Wiederholte Punktnummern benötigen eine eindeutige Zeilenidentität.
4. Nur tatsächlich ältere Dateien ohne gespeicherte Aktivierungen dürfen dokumentierte Standardwerte erhalten. Vorhandene, aber unpassende Aktivierung nicht stillschweigend als „alles aktiv“ behandeln: Warnung und explizite Entscheidung oder Sperre.
5. Bei bekanntem Standpunkt den passenden Berechnungsweg mit fester Lage und gegebenenfalls fester Höhe verwenden. Mehrere Anschlussziele machen daraus keine freie Stationierung.
6. Eine bewusst geänderte Beobachtungsauswahl als neue Berechnungsrevision behandeln. Die historische Stationsdatei nicht allein durch Öffnen oder Vorschau überschreiben.

### Regression und Abnahme

- Gespeicherten ausgeschalteten Grobfehler im Polarmodul öffnen: Haken bleiben ausgeschaltet und er beeinflusst die neue Rechnung nicht.
- Dieselben Daten ohne Änderungen laden: gespeichertes und nachgerechnetes Ergebnis stimmen innerhalb dokumentierter Toleranzen überein, sofern dieselben Parameter gelten.
- Bekannter Standpunkt mit einem sowie mehreren Zielen: feste Koordinaten bleiben fest.
- Wiederholungen derselben Zielnummer, fehlende Legacy-Haken, unpassende Listen und Aufstellungswechsel testen.

**Erledigt, wenn:** Die Quelle „gespeichert“ ihre wirksamen Bedingungen erhält und jede bewusste Abweichung erkennbar als neue Rechnung behandelt wird.

**Abschlussnachweis:** _Commit, Verhalten der beiden Bedienaktionen und Regressionen ergänzen._

## P3 – Exzentrizitätskorrekturen über stabile Messidentitäten schreiben

**Priorität:** Hoch. **Nachweis:** Mit Originalfunktionen der Arbeitskopie ausgeführt.

**Fundstellen:** `src/daten/polar_messdaten.rs:95–100`, `src/gui/polarpunkte.rs:942–953`, `src/daten/messdaten.rs:294–308`.

**Fehlerbild:** KP-Zeilen fehlen in der gefilterten Polarliste. Nach dem ersten Schreiben wird diese Liste trotzdem mit sämtlichen Dateizeilennummern verknüpft. Bei `NP1, KP, NP2` trifft die zweite Exzentrizitätskorrektur von NP2 die KP-Zeile.

### Lösungsschritte

1. Die Paarung einer gefilterten Messliste mit sämtlichen Dateizeilen entfernen. Physische Zeilennummern nur als Anzeige- oder Diagnosewert verwenden, nicht als dauerhafte Identität.
2. Jedem Messdatensatz eine stabile Kennung zuordnen. Eine Kombination aus Punktnummer und Aufstellung allein reicht bei Wiederholungsmessungen nicht. Für vorhandene Dateien eine eindeutige ID-Migration oder eine bestätigte Zuordnung einschließlich Wiederholungskennung vorsehen.
3. Beim Laden eine Dateirevision oder einen Inhaltsfingerprint erfassen. Beim Schreiben dieselbe Revision erwarten. Wenn Dateiinhalt oder Reihenfolge extern geändert wurden, Neu-Laden und Zusammenführen verlangen, statt anhand alter Indizes zu schreiben.
4. Alle gewünschten Änderungen zunächst im vollständigen CSV-Dokument anhand der stabilen IDs auflösen. Fehlende oder mehrdeutige Ziele vor dem ersten Schreibzugriff melden. KP, AP und nicht ausgewählte NP unverändert lassen.
5. Das geänderte Dokument einmal atomar schreiben und anschließend anhand der IDs neu einlesen. Erst nach Erfolg den internen Originalvergleich und die Revision aktualisieren.
6. Berechnete Punkte und korrigierte Rohdaten gemeinsam nach Q1 behandeln. Wenn die CSV nicht gespeichert wurde, dürfen darauf beruhende Koordinaten nicht als vollständig synchronisiert gelten.

### Regression und Abnahme

- Vorhandenen Fall `NP1, KP, NP2` zweimal korrigieren: nur NP2 trägt zuletzt 2,000 m; KP bleibt unverändert.
- Mehrere KP/AP zwischen NP, Kommentare, leere Zeilen, unterschiedliche Zeilenenden und mehrzeilige zitierte CSV-Felder prüfen.
- Gleiche Punktnummer mehrfach in derselben Aufstellung: nur der ausgewählte Datensatz wird geändert.
- Extern eingefügte beziehungsweise umsortierte Zeile und gesperrte CSV führen zu einem Konflikt beziehungsweise Fehler ohne Änderung anderer Daten.

**Erledigt, wenn:** Beliebig oft wiederholte Korrekturen stets genau die ausgewählten Messungen treffen und ein externer Dateikonflikt nicht stillschweigend übergangen wird.

**Abschlussnachweis:** _Commit, Datensatz-ID-/Migrationsregel und Schreibkonflikttests ergänzen._

## P4 – Fehlende Anschlussgrundlagen dürfen keine lokale Ersatzstation erzeugen

**Priorität:** Hoch. **Nachweis:** Mit Originalfunktionen der Arbeitskopie ausgeführt.

**Fundstellen:** `src/gui/polarpunkte.rs:465–470, 504–508, 617–628, 686–705`.

**Fehlerbild:** Nach dem Überspringen aller nicht auflösbaren AP bleibt die Liste leer. Das wird als lokaler Fall behandelt. Der Prüfstand erzeugte aus unvollständigen Grundlagen einen lokalen Punkt bei R=0, H=10 m, der nach Hinweisbestätigung speicherbar ist.

### Lösungsschritte

1. Die Zustände **ausdrücklich lokal**, **stationiert**, **Grundlagen fehlen**, **Zuordnung unklar** und **Berechnung fehlgeschlagen** fachlich unterscheiden. Nicht aus der Länge der verbliebenen Anschlussliste auf den Modus schließen.
2. Beim Einlesen festhalten, ob AP erwartet wurden und welche nicht aufgelöst werden konnten. Die Ursache muss nach dem Filtern erhalten bleiben.
3. Bei erwarteten, aber fehlenden Grundlagen die betroffene Stationierung blockieren oder nur einen ausdrücklich freigegebenen, fachlich begründeten Teilbestand verwenden. Vollständiges Fehlen darf nicht automatisch auf Ursprung und Orientierung 0 führen.
4. Eine lokale Berechnung nur durch bewusste Auswahl beziehungsweise eindeutige lokale Projekt-/Aufstellungsmetadaten aktivieren. Bei einem Wechsel aus einem übergeordneten System neue Herkunft und Konsequenzen bestätigen lassen.
5. Ergebnisse mit ihrem Koordinatenbezug kennzeichnen. Speicherung, Festpunktübernahme und Export müssen denselben Bezug führen; der Name eines Punktes macht lokale Koordinaten nicht zu übergeordneten Koordinaten.
6. Bei fehlerhafter Neuberechnung frühere Ergebnisse sichtbar entwerten. Alte Koordinaten nicht im aktuellen Fenster als neue lokale Lösung weiterverwenden.

### Regression und Abnahme

- Vorhandene DAT mit zwei AP und fehlendem Festpunktbestand erzeugt keinen speicher- oder exportierbaren Ersatzpunkt.
- Eine nicht gefundene Stationsdatei, falsche Aufstellung und eine teilweise fehlende AP-Liste behalten ihren Fehlerstatus.
- Ein bewusst angelegtes lokales Projekt funktioniert weiterhin und ist in Anzeige sowie Ausgabe eindeutig lokal gekennzeichnet.
- Nach Wiederherstellung der Grundlagen ist eine neue, normale Berechnung möglich; keine Vermischung mit vorherigen lokalen Ergebnissen.

**Erledigt, wenn:** Leere oder unvollständige Grundlagen niemals ohne ausdrückliche Modusentscheidung in einen anderen Koordinatenbezug umgedeutet werden.

**Abschlussnachweis:** _Commit, Zustandsmodell und Referenzfälle ergänzen._

---

## D1 – Räumliche Löschung auf vollständige Punktschlüssel begrenzen

**Priorität:** Hoch. **Nachweis:** Mit Originalfunktionen der Arbeitskopie ausgeführt.

**Fundstellen:** `src/gui/dxf_viewer/zeichnen.rs:927–941, 971–973`, `src/daten/punktlisten.rs:289–297, 363–368, 412–419`.

**Fehlerbild:** Die räumliche Auswahl und `LoeschZiele` verkürzen den Schlüssel auf Punkt- und Standpunktnummer. Dadurch werden gleich nummerierte Messungen anderer Aufstellungen mitgelöscht. Rohdaten ohne Koordinate können wegen eines 0/0-Ersatzwerts zusätzlich unsichtbar in die Auswahl geraten.

### Lösungsschritte

1. In der Trefferermittlung die tatsächlichen Koordinatendatensätze einschließlich AufstellungId verwenden. Fehlende Koordinate als fehlend behandeln; keinen Ursprungspunkt erfinden.
2. `LoeschZiele` auf vollständige typisierte Referenzen umstellen: Bestandsart, Punktnummer, Standpunktnummer und AufstellungId; bei einem zukünftigen feineren Messschlüssel auch dessen Kennung mitführen.
3. Aufstellungsgenaue Koordinatensuche und Löschbedingungen verwenden. Die allgemeine Suche `finde_koordinate(nr, sp)` darf diesen Ablauf nicht mehr bestimmen.
4. Auswahlvorschau und Bestätigung aus denselben eingefrorenen Zielen aufbauen wie die spätere Ausführung. Mehrdeutige oder nicht sichtbare Bestände nicht stillschweigend hinzunehmen.
5. Vor dem Schreiben prüfen, ob Bestand beziehungsweise Revision noch zur Vorschau passt. Eine währenddessen geänderte Datei erfordert eine neue Auswahlprüfung.
6. Nach Q1 Result-Fehler weitergeben und erst tatsächlich ausgeführte Änderungen als gelöscht protokollieren. Bei mehreren Beständen Teilerfolg konkret benennen oder einen gemeinsamen Commit verwenden.

### Regression und Abnahme

- Gleiche Punkt- und Standpunktnummer in A und B; nur A im Rechteck: genau A wird gelöscht.
- Nur B im Rechteck und umgekehrte Dateireihenfolge: Ergebnis bleibt korrekt.
- Unkoordinierte Rohdaten bleiben bei einer Auswahl um 0/0 erhalten.
- Vorschauanzahl entspricht den eindeutigen tatsächlich betroffenen Schlüsseln.
- Dateisperre oder Konflikt darf weder falschen Erfolg noch falsches Löschprotokoll erzeugen.

**Erledigt, wenn:** Der vorhandene Audit-Test die zweite Aufstellung nach der Löschung unverändert vorfindet und keine Auswahlentscheidung mehr auf einem verkürzten Schlüssel oder einer erfundenen Koordinate beruht.

**Abschlussnachweis:** _Commit, Auswahl-/Löschtests und Fehlerpfad ergänzen._

## D2 – Picking und Punktnummer müssen denselben tatsächlichen Treffer bezeichnen

**Priorität:** Hoch. **Nachweis:** Falsche Markerwahl ausgeführt; Endpunkt-Nummernfehler im Codepfad belegt.

**Fundstellen:** `src/gui/dxf_viewer.rs:846–883`, `src/gui/dxf_viewer/zeichnen.rs:809–820, 899, 907–923`.

**Fehlerbild:** Der erste Marker im 20-Pixel-Suchkreis verdrängt nähere Kandidaten. Bei ausgeschalteten Markern kann der zweite Endpunkt mit der Nummer des ersten Endpunkts verknüpft werden.

### Lösungsschritte

1. Alle zulässigen Kandidaten erfassen und gemeinsam nach **Bildschirmdistanz** bewerten. Den vorzeitigen Rücksprung für den ersten Marker entfernen. Die Reihenfolge in der DXF-Datei darf kein Entscheidungskriterium sein.
2. Zulässige Kandidaten klar festlegen: sichtbare Layer, aktuell wählbare Bestandsarten, Punkt-/Endpunkt-/Objekttreffer und aktiver Snapmodus. Nicht dargestellte oder gesperrte Ersatzgeometrie ausschließen.
3. Ein strukturiertes Trefferergebnis zurückgeben: Weltkoordinate, Quellobjekt, tatsächlicher Endpunkt beziehungsweise Parameter, Punktkennung und Bildschirmdistanz. Die Oberfläche darf anschließend keine Nummer aus einem anderen Punkt rekonstruieren.
4. Fachliche Prioritäten, etwa zwischen überlagertem Festpunkt und Planmarker, explizit festlegen. Eine weiter entfernte Identität nicht allein wegen ihrer Bestandsart stillschweigend bevorzugen.
5. Bei nicht eindeutig unterscheidbaren Treffern eine Auswahl mit Punktnummer, Herkunft und Koordinaten anbieten. Ein stabiler technischer Tie-Breaker darf keine fachliche Eindeutigkeit vortäuschen.
6. Beim Linienendpunkt die Nummer aus genau der gefangenen Koordinate beziehungsweise der stabilen Endpunktreferenz beziehen. Gleiche Behandlung bei eingeschalteten und ausgeschalteten Markern sicherstellen. Den Fangradius auf die tatsächliche Fangkoordinate anwenden: Eine Linie unter dem Mauszeiger berechtigt nicht zum Fang eines weit entfernten Endpunkts. Ein freier Punkt im Linieninneren erhält ohne entsprechenden Snap keine Endpunktnummer.

### Regression und Abnahme

- Marker A liegt 1 Pixel neben B, Klick exakt B: B gewinnen lassen, unabhängig von Entityreihenfolge.
- Nähere Import-/Festpunkte werden nicht von einem weiter entfernten DXF-Marker verdrängt.
- Für beide Endpunkte einer Linie jeweils Koordinate und Punktnummer prüfen, mit und ohne Markeranzeige.
- Klick in die Linienmitte ohne Snap übernimmt keine Endpunktnummer. Ein außerhalb des Fangradius liegender Endpunkt wird auch bei einer Linie unter dem Mauszeiger nicht gefangen.
- Zoom, DPI-Skalierung und Touch-Suchradius variieren; Suchdistanz bleibt eine konsistente Bildschirmgröße.
- Bei überlagerten unterschiedlichen Identitäten wird die Mehrdeutigkeit sichtbar gelöst.

**Erledigt, wenn:** Der vorhandene Picking-Gegentest besteht und der Anschlussmessdialog immer dieselbe Identität übernimmt, die der Benutzer tatsächlich ausgewählt hat.

**Abschlussnachweis:** _Commit, Auswahlregeln, Tests und Touch-Bedienprüfung ergänzen._

## D3 – Nicht unterstützte DXF-Geometrie darf keine präzise Ersatzgeometrie vortäuschen

**Priorität:** Hoch **bei Plänen mit betroffenen Eigenschaften**. **Nachweis:** Codepfad und DXF-Formatabgleich.

**Fundstellen:** `src/dxf/leser.rs:141–187`, `src/dxf/entity.rs:401–410`.

**Fehlerbild:** `LWPOLYLINE`-Bulges und nichtstandardmäßige Extrusionsrichtungen werden nicht verarbeitet. Gekrümmte Teilstücke erscheinen dadurch gerade beziehungsweise OCS-Koordinaten werden ohne passende Transformation verwendet. `INSERT` stellt derzeit einen Einfügepunkt dar, keine aufgelöste Blockgeometrie.

### Lösungsschritte

1. Einen verbindlichen Importvertrag erstellen: unterstützte Entitytypen, Eigenschaften, Koordinatensysteme und Einschränkungen. Unter anderem Bulge (Code 42), OCS/Extrusion (210/220/230), geschlossene Polylinien, Erhebung (Code 38) sowie Blockreferenzen berücksichtigen.
2. Beim Import alle erkannten, unterstützten und verworfenen Eigenschaften zählen und betroffene Entity-Handles beziehungsweise auffindbare Referenzen melden. Eine allgemeine Bemerkung in der Dokumentation genügt nicht für die Entscheidung über einen konkreten Plan.
3. Für jede Eigenschaft eine explizite Umsetzung wählen: **korrekt unterstützen** oder **betroffene Geometrie erkennbar ablehnen beziehungsweise als unzuverlässige Vorschau sperren**. Die sichere begrenzte Unterstützung ist ein zulässiger erster Abschluss; keine stillschweigende Gerade anstelle eines Teilbogens verwenden. Bei Ablehnung der Datei müssen bisherige Zeichnung, Punktindex, Ansicht und Arbeitsmodus erhalten bleiben; keine nebenbei ausgelöste Umschaltung in ein lokales System.
4. Bei Bulge-Unterstützung jedes Segment einschließlich des Schließsegments behandeln. Vorzeichen, Bogenorientierung, Null-Bulge, negative Bulges, große Bögen und degenerierte Endpunkte berücksichtigen. Die Geometrie muss für Anzeige, Bounds, Snap und Export dieselbe Bedeutung besitzen.
5. Bei OCS-Unterstützung Entity-Koordinaten einschließlich Extrusionsrichtung und relevanter Höhe vor der Welt-/Planprojektion korrekt transformieren. Nichtstandardmäßige Eigenschaften bis zur vollständigen Unterstützung sperren statt Komponenten zu ignorieren.
6. Für `INSERT` unterscheiden, ob nur ein bewusst angebotenes Symbolzentrum oder tatsächliche Blockgeometrie verwendet wird. Bei fehlender Blockauflösung darf die Oberfläche nicht suggerieren, die Blockkontur sei vermessungstauglich ausgewertet. Vollständige Unterstützung müsste auch Skalierung, Rotation, Verschachtelung und Einheiten berücksichtigen.
7. Nicht unterstützte Vorschauobjekte von präzisem Picking, Digitalisierung und abgeleiteten Koordinatenausgaben ausschließen. Bestätigung einer Warnung allein macht eine Ersatzgeometrie nicht korrekt.

### Regression und Abnahme

- Referenzpläne für gerades Segment, positive/negative Bulges, geschlossene Polylinie, große Bögen, OCS-Normalen und Blockreferenzen anlegen.
- Bulge-Beträge größer als 1 sowie Normalen `(0,0,-1)` und geneigte Ebenen prüfen. Fehlende Extrusionscodes verwenden den dokumentierten Standard `(0,0,1)`; ungültige oder nicht endliche Werte werden abgelehnt.
- Erwartete Endpunkte, Bogenlagen und Weltkoordinaten unabhängig bestimmen und gegen Anzeige-/Snap-Geometrie prüfen.
- Im begrenzten Unterstützungsmodus erzeugen diese Eigenschaften konkrete Importdiagnosen; betroffene Ersatzobjekte liefern keine präzisen Pick-Ergebnisse.
- Eine abgelehnte Datei lässt den zuvor geladenen Plan und Punktindex unverändert.
- Normale vollständig unterstützte Pläne funktionieren unverändert.

**Erledigt, wenn:** Für jede behandelte Eigenschaft entweder die korrekte Geometrie mit Tests vorliegt oder ein dokumentierter, getesteter Ausschluss greift. Der Importstatus eines konkreten Plans ist für den Benutzer erkennbar.

**Formatreferenz:** [Autodesk DXF Reference – LWPOLYLINE](https://help.autodesk.com/cloudhelp/2026/DEU/AutoCAD-DXF/files/GUID-748FC305-F3F2-4F74-825A-61F04D757A50.htm). Weitere unterstützte Entitytypen anhand ihrer jeweiligen Formatdefinition umsetzen.

**Abschlussnachweis:** _Commit, unterstütztes Profil, Referenzpläne und Importdiagnosen ergänzen._

---

## Q1 – Zusammengehörige Änderungen müssen fachlich konsistent abgeschlossen werden

**Priorität:** Hoch. **Nachweis:** Mehrere betroffene Produktionspfade im Code belegt.

**Fundstellen:**

- `src/daten/projekt.rs:941–953, 1080–1127`: Stationsübernahme und Neupunktneuberechnung.
- `src/gui/dxf_viewer/zeichnen.rs:453–461, 963–986`: Digitalisierung und Löschen.
- `src/gui/polarpunkte.rs:742–782, 897–960`: Punktübernahme und Exzentrizitätsrückschreiben.

**Fehler und Folge:** Einzeldateien werden geschützt, aber mehrere zusammengehörige Dateien nacheinander geändert. Fehler können nur im Log landen oder als Anzahl 0 verschluckt werden. Neue Stationen können neben alten Neupunkten erscheinen, eine fehlgeschlagene Löschung kann als ausgeführt protokolliert werden, und ein teilweise geschriebener KOR-/CSV-Export kann beim Wiederholen Duplikate erzeugen.

### Lösungsschritte

1. Für jeden Vorgang die zusammengehörigen Daten definieren: Station übernehmen, IH korrigieren, Polarpunkte samt Rohkorrekturen speichern, Bestände löschen, Digitalisierung als KOR/CSV schreiben. Jeweils Eingangsdaten, Zieldateien, abgeleitete Daten und Protokollereignisse auflisten.
2. Entscheiden, welcher Bestand kanonisch ist. Abgeleitete Dateien nach Möglichkeit daraus regenerieren. Wo mehrere kanonische Dateien erforderlich bleiben, einen vorbereiteten, wiederaufnehmbaren Commit mit Journal oder eine gleichwertige Transaktionsstrategie entwerfen. Mehrere atomare `rename`-Aufrufe sind noch keine Gesamttransaktion.
3. Eine persistierte Stations-/Parameterrevision einführen. Berechnete Koordinaten müssen angeben, aus welcher Aufstellung und Revision sie stammen. Ein bloßer GUI-Zähler im Speicher reicht nach Neustart nicht zur Konsistenzprüfung.
4. Vor dem ersten Commit Berechnung und Validierung abschließen und erwartete Dateirevisionen prüfen. Neue Inhalte zunächst vorbereiten. Schreibsperren, atomare Einzeldateiersetzung und Backup weiterhin verwenden.
5. Nach Fehlern einen definierten Zustand herstellen: entweder den vollständigen vorherigen Zustand wiederherstellen oder einen ausdrücklich markierten, nach Neustart wiederaufnehmbaren Teilzustand speichern. Im zweiten Fall alte Koordinaten für aktuelle Verwendung und Export als veraltet sperren, bis sie erfolgreich neu berechnet sind.
6. `neupunkte_nach_stationierung` und vergleichbare Funktionen müssen Fehler an den Aufrufer zurückgeben. `unwrap_or(0)` bei schreibenden Resultaten entfernen. Erfolgston, Status, Zähler und Übernahmebestätigung an das tatsächliche Ergebnis des Gesamtvorgangs binden.
7. Auditereignisse unterscheiden: angefordert, erfolgreich abgeschlossen, fehlgeschlagen beziehungsweise teilweise abgeschlossen. Eine Löschung erst nach nachgewiesenem Commit als ausgeführt dokumentieren; betroffene Schlüssel und Revisionen erfassen.
8. Polarpunkte gesammelt statt pro Punkt als vollständig neuen Bestand schreiben. Vor Erfolgsmeldung auch das Rückschreiben der Rohkorrekturen berücksichtigen. Ein Protokollfehler darf einen bereits wirksamen Datencommit nicht verstecken; Daten- und Protokollstatus getrennt und zutreffend anzeigen.
9. Digitalisierung idempotent machen: aus einem kanonischen Datensatz beide Ausgaben erzeugen oder eine stabile Vorgangskennung verwenden. Wiederholen nach einem Fehler darf dieselbe Messung nicht nochmals anhängen.
10. Beim Laden auf unterbrochene Vorgänge, abweichende Revisionen und externe Dateiveränderungen prüfen. Ein Journal darf nicht ungeprüft eine inzwischen extern geänderte Datei überschreiben.

### Regression und Abnahme

- Für jeden schreibenden Teilschritt einen Fehler injizieren: erste Datei erfolgreich, folgende Datei gesperrt; Protokoll nicht schreibbar; Neupunktneuberechnung fehlgeschlagen.
- Prozess zwischen Vorbereitung und einzelnen Commit-Schritten gezielt beenden und Projekt erneut laden. Zustand muss konsistent oder ausdrücklich reparierbar sein.
- Neue Station plus alte Neupunkte darf nie als aktueller vollständiger Bestand exportiert werden.
- Fehlgeschlagene Löschung hinterlässt keine falsche Erfolgsmeldung und kein uneingeschränktes „gelöscht“-Ereignis.
- Gesperrte Begleit-CSV nach erfolgreichem KOR-Schreibversuch erzeugt bei Wiederholung keine doppelte Punktzeile.
- Extern geänderte Datei beziehungsweise Nextcloud-Konflikt als Revisionskonflikt behandeln, nicht als normale erfolgreiche Speicherung.

**Erledigt, wenn:** Jeder genannte Vorgang bei Erfolg, Teilfehler, Abbruch und Neustart einen definierten, nachvollziehbaren Zustand besitzt. Statusmeldungen und Audit stimmen mit dem gespeicherten Bestand überein.

**Abschlussnachweis:** _Transaktionsentscheidung, Commit, Fehler-Injektionsmatrix, Neustarttests und Wiederherstellungsablauf ergänzen._

---

## Gemeinsame Test- und Freigabestrategie

### Vorhandene Prüfbelege übernehmen

Die Dateien der Analyse liegen ursprünglich unter:

```text
C:\Users\hans\Documents\GIT-Projekte\Feldbuch-NT-Analyse\
```

| Beleg | Geeignet für |
|---|---|
| `communication-probe/probe.rs`, `result.txt` | K1 und Kommunikationsgegenproben; echte Zustandsmaschinen, teilweise nachgebildete abhängige Datentypen |
| `station-probe/station_probe.rs`, `.log` | S1 und S2; Einbindung des unveränderten Rechenkerns |
| `station-probe/ih_probe.rs`, `IH-Gegenprobe.log` | S3; vollständige Kette aus Station, Neupunkt und IH-Korrektur |
| `polar_audit_tests.rs`, `Polar-Gegenproben.log` | P1 bis P4; vier ausgeführte Gegenproben |
| `dxf-probe/viewer_audit.rs`, `DXF-Gegenproben.log` | D1 und D2; Löschen über Aufstellungen und falsche Markerwahl |
| `Quellstand_SHA256.json`, `Quellstand_Pruefung.json` | Abgrenzung des damals geprüften Quellstands |

**Wichtig:** Die eigenständigen Prüfstände nicht ungeprüft als dauerhafte zweite Implementierung übernehmen. Relevante Fälle an echte Produktionsschnittstellen anbinden und erforderliche Stubs auf ihre Annahmen prüfen. Die Analysekopie enthält zusätzliche Testmodule und ist kein reparierter Entwicklungsstand.

### Testreihenfolge

1. Neue Regression des jeweiligen Pakets ausführen und den ursprünglichen Fehler nachweisen.
2. Nach Korrektur gezielte Tests des betroffenen Moduls und seiner Aufrufer ausführen.
3. Gemeinsamen Ablauf prüfen: Gerät konfigurieren → Stationierung → Aufnahme → Polarpunkte → Speicherfehler/Neustart → Export.
4. Gesamtsuite und Build in der tatsächlichen Projektkonfiguration ausführen. Cargo aus dem Repository starten, damit `.cargo/config.toml` berücksichtigt wird:

```powershell
cargo test --locked --no-fail-fast
cargo build --locked
```

Bei vorhandenem Paketcache kann ergänzend `--offline` verwendet werden. Plattformbedingte Rechteprobleme von tatsächlichen Testfehlern unterscheiden. Falls Geräteversuche oder interaktive Bedienprüfungen noch fehlen, den Paketstatus entsprechend als teilweise geprüft kennzeichnen.

### Abschluss pro Paket dokumentieren

```text
ID:
Bearbeiter und Datum:
Implementierungsentscheidung:
Geänderte Produktionspfade:
Commit:
Neue oder angepasste Regressionstests:
Ergebnis der relevanten Tests:
Ergebnis des gemeinsamen Arbeitsablaufs:
Gerätemodell/Firmware und Geräteprüfung, falls erforderlich:
Migration oder Wiederherstellung vorhandener Daten:
Verbleibende Einschränkungen:
Abschlusskriterien erfüllt: ja/nein
```

Historische Messungen nicht automatisch als falsch erklären. Für bereits erzeugte Daten anhand von Aufstellung, verwendeter Konfiguration und betroffener Funktion feststellen, welche Ergebnisse überprüft oder neu berechnet werden müssen. Nachträgliche Korrekturen mit Bezug auf den ursprünglichen Datensatz protokollieren.

---

## Umsetzungsstand (23.11.2026, Arbeitskopie nach `b5a48fc`, noch nicht committet)

Gesamtsuite: **410 Tests bestanden** (403 Baseline + 7 neue). Oberfläche und Gerät nicht interaktiv geprüft.

```text
ID: S1
Implementierungsentscheidung: Helmert-Näherung und Maßstab nur aus Zielen mit aktiver Richtung UND Strecke.
  Weniger als 2 solche Ziele: freier Maßstab → Fehler „Maßstab nicht bestimmbar“; fester Maßstab → Näherung
  nur als Startwert (Maßstab 1, Ergebnis allein aus aktiven Beobachtungen der Ausgleichung).
Geänderte Produktionspfade: src/rechnen/freie_stationierung.rs (berechnen, Schritt 1)
Neue Regressionstests: deaktiviertes_ziel_beeinflusst_kein_ergebnis, abgeschaltete_strecke_beeinflusst_den_massstab_nicht,
  freier_massstab_braucht_zwei_vollstaendige_ziele
Gegenprobe: alle drei scheitern mit dem alten Schritt 1, bestehen mit dem neuen.
Verbleibende Einschränkungen: Modellfrage Maßstab/Ausgleichung (S2) zurückgestellt.
Abschlusskriterien erfüllt: ja (für S1)

ID: S3
Implementierungsentscheidung: Stationshöhe aus Anschlusszielen (freie Station 3D, bekannter Punkt ohne feste Höhe)
  wird um −ΔIH korrigiert (exakt, ohne Neuberechnung mit evtl. geänderten Parametern); feste Höhe eines bekannten
  Standpunkts bleibt, Neupunkte ändern sich dann um +ΔIH; 2D unverändert. Feldbuchpunkt des Standpunkts und
  Projektdaten werden nachgeführt; Rückfrage nennt die Wirkung (Projekt::ih_korrektur_wirkung);
  Fehler der Neupunktberechnung werden zurückgegeben statt nur protokolliert.
Geänderte Produktionspfade: src/daten/projekt.rs (set_instrumenten_hoehe, neupunkte_neu_berechnen),
  src/gui/dxf_viewer/standpunkt.rs, src/gui/freie_stationierung.rs (Rückfragetexte)
Neue Regressionstests: ih_korrektur_erhaelt_neupunkthoehen_bei_freier_station (98,500 → 98,350; NP bleibt 100,000;
  Rück- und Mehrfachkorrektur), ih_korrektur_bei_fester_standpunkthoehe
Gegenprobe: der Test der freien Station scheitert ohne die Korrektur.
Verbleibende Einschränkungen: echte Änderung des Instrumentaufbaus muss weiterhin über „Neuer Stp“ laufen (Hinweis im Dialog).
Abschlusskriterien erfüllt: ja

ID: D1 (mit Q1-Teil Löschprotokoll)
Implementierungsentscheidung: Auswahl über NeupunkteManager::schluessel_wo mit vollständigem Schlüssel
  (Punkt, Standpunkt, AufstellungId) und tatsächlicher Koordinate; Rohdaten ohne Koordinate sind nicht wählbar.
  Löschen über NeupunkteManager::entferne. Fehler werden angezeigt statt mit unwrap_or(0) verschluckt;
  GELOESCHT wird erst nach erfolgreichem Löschen und nur für tatsächlich gelöschte Bestände protokolliert.
Geänderte Produktionspfade: src/daten/punktlisten.rs, src/gui/dxf_viewer/zeichnen.rs (rechteck_gewaehlt, loesche,
  protokolliere_loeschen), src/gui/dxf_viewer.rs (LoeschZiele)
Neue Regressionstests: loeschen_trifft_nur_die_gewaehlte_aufstellung
Gegenprobe: nicht als Vorher-Test möglich (neue Schnittstelle); der Testfall entspricht dem Audit-Fall.
Verbleibende Einschränkungen: Feldbuch- und Importpunkte weiter nach Punktnummer; keine Revisionsprüfung zwischen
  Vorschau und Ausführung.
Abschlusskriterien erfüllt: teilweise (Neupunkte ja)

ID: P3
Implementierungsentscheidung: PolarMessung::datensatz = Index unter allen Datenzeilen (auch AP/KP); Rückschreiben
  über messdaten::ExzAenderung mit Prüfung von Punktnummer und Zeitstempel; erst alle Ziele prüfen, dann einmal
  speichern; bei Abweichung Konflikt ohne Schreiben. Die fehlerhafte Neuzuordnung (zip über gefilterte Liste) entfällt.
Geänderte Produktionspfade: src/daten/polar_messdaten.rs, src/daten/messdaten.rs, src/gui/polarpunkte.rs
Neue Regressionstests: exzentrizitaet_trifft_nur_den_gewaehlten_neupunkt (NP1, KP, NP2; zweimal; Konfliktfall)
Gegenprobe: der Fehler lag in der GUI-Neuzuordnung; als Vorher-Test nicht direkt ausführbar.
Verbleibende Einschränkungen: keine dateiweite Revision; Fingerprint je Zeile (Punktnummer + Zeitstempel).
Abschlusskriterien erfüllt: ja (mit dieser Einschränkung)
```
