# CSV-Beispiele

- `Demo-kor.csv`: Koordinatenimport im Viewer, in der Transformation oder Flächenteilung. Punkt 002 hat keine bekannte Höhe; der Code enthält ein Semikolon.
- `Demo-dat.csv`: Messdaten für das Polarmodul. SP01 hat zwei getrennte Aufstellungen mit unterschiedlicher Instrumentenhöhe. Für eine reale Berechnung sind passende Anschlusskoordinaten und eine gültige Stationierung erforderlich.
- `Demo-SP01-stp.csv`: Anschlusskoordinaten und Beobachtungen einer Aufstellung. Die freie Stationierung übernimmt die AP-Zeilen; die NP-Zeile bleibt im CSV-Dokument erhalten.

Alle drei Dateien lassen sich im Datenmanager öffnen. Neue Dateien über „Neu KOR“, „Neu DAT“ oder „Neu STP“ anlegen. Beim Schreiben gelten UTF-8, Semikolon und Dezimalpunkt; der Import akzeptiert auch bisherige CSV-Dateien.

Die Beispiele dienen der Formaterklärung. Ein STP-Ergebnisabschnitt ersetzt keine vollständige Projektsicherung mit Rechenparametern und Beobachtungsaktivierungen.
