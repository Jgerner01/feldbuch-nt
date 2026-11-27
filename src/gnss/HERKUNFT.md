# Herkunft des GNSS-Moduls

Übernommen aus **gnss-vermessung 0.4.0** (Commit `7526a36`, Repository `Jgerner01/gnss-vermessung`) am 26.11.2026.
Ab der Übernahme wird der Code hier **eigenständig** gepflegt; gnss-vermessung bleibt unverändert und wird getrennt weiterentwickelt.
Verbesserungen werden nur bewusst und von Hand übertragen und unten unter „Übertragene Änderungen“ vermerkt.

Der Projektordner des Originals heißt seit 27.11.2026 **`GNSS-NT`** (vorher `gnss-vermessung`); Paketname,
Programmdateien, Einstellungsordner `%APPDATA%\gnss-vermessung` und Repository sind unverändert.

Modulpfade wurden angepasst (`crate::` → `crate::gnss::kern::` bzw. `crate::gnss::ansicht::`), jede Datei trägt einen Herkunftsvermerk im Kopf.

## Dateien

| Feldbuch-NT | Ursprung | Änderungen bei der Übernahme |
|---|---|---|
| `kern/config.rs` | `src/config.rs` | nur Modulpfade |
| `kern/demux.rs` | `src/demux.rs` | nur Modulpfade |
| `kern/dop.rs` | `src/dop.rs` | nur Modulpfade |
| `kern/engine.rs` | `src/engine.rs` | `configure_persistent`: Flash lesen, nur geänderte Schritte dauerhaft schreiben; Konfigurationsablauf in `run_config` ausgelagert |
| `kern/export.rs` | `src/export.rs` | nur Modulpfade |
| `kern/geo.rs` | `src/geo.rs` | nur Modulpfade |
| `kern/gtime.rs` | `src/gtime.rs` | nur Modulpfade |
| `kern/height.rs` | `src/height.rs` | nur Modulpfade |
| `kern/measure.rs` | `src/measure.rs` | nur Modulpfade |
| `kern/model.rs` | `src/model.rs` | Feld `persisted_steps` |
| `kern/nmea.rs` | `src/nmea.rs` | nur Modulpfade |
| `kern/ntrip.rs` | `src/ntrip.rs` | nur Modulpfade |
| `kern/rtcm.rs` | `src/rtcm.rs` | nur Modulpfade |
| `kern/rtcm_decode.rs` | `src/rtcm_decode.rs` | nur Modulpfade |
| `kern/settings.rs` | `src/settings.rs` | eigene Datei `%LOCALAPPDATA%\Feldbuch\gnss.toml`, Tresor-Dienst `feldbuch-nt-sapos`, einmalige Übernahme aus dem Original, `check_updates` entfernt |
| `kern/spp.rs` | `src/spp.rs` | nur Modulpfade |
| `kern/survey.rs` | `src/survey.rs` | nur Modulpfade |
| `kern/transform.rs` | `src/transform.rs` | nur Modulpfade |
| `kern/ubx.rs` | `src/ubx.rs` | `gui_config_items`, `valget_layer`, `VALGET_RAM/FLASH` |
| `kern/ubxmsg.rs` | `src/ubxmsg.rs` | nur Modulpfade |
| `kern/version.rs` | `src/version.rs` | neu geschrieben: Versionsangabe von Feldbuch-NT |
| `ansicht/app.rs` | `src/bin/gnss-gui/app.rs` | eingebettet statt `eframe::App`; Update-Funktion und Bildschirmfotos entfernt; Übernahmedialog; dauerhafte Konfiguration beim Verbinden; Schnittstelle für Feldbuch-NT (`live`, `status`, `start_extern` …) |
| `ansicht/map_stations.rs` | `src/bin/gnss-gui/map_stations.rs` | nur Modulpfade |
| `ansicht/tiles.rs` | `src/bin/gnss-gui/tiles.rs` | nur Modulpfade |
| `ansicht/view_config.rs` | `src/bin/gnss-gui/view_config.rs` | Update-Teil der Hilfe und Darstellung (Farbschema, Vergrößerung) entfernt |
| `ansicht/view_dop_anim.rs` | `src/bin/gnss-gui/view_dop_anim.rs` | nur Modulpfade |
| `ansicht/view_ephem.rs` | `src/bin/gnss-gui/view_ephem.rs` | nur Modulpfade |
| `ansicht/view_map.rs` | `src/bin/gnss-gui/view_map.rs` | nur Modulpfade |
| `ansicht/view_measure.rs` | `src/bin/gnss-gui/view_measure.rs` | nur Modulpfade |
| `ansicht/view_ntrip.rs` | `src/bin/gnss-gui/view_ntrip.rs` | nur Modulpfade |
| `ansicht/view_raw.rs` | `src/bin/gnss-gui/view_raw.rs` | nur Modulpfade |
| `ansicht/view_sky.rs` | `src/bin/gnss-gui/view_sky.rs` | nur Modulpfade |
| `ansicht/view_status.rs` | `src/bin/gnss-gui/view_status.rs` | nur Modulpfade |
| `ansicht/view_time.rs` | `src/bin/gnss-gui/view_time.rs` | nur Modulpfade |
| `ansicht/view_track.rs` | `src/bin/gnss-gui/view_track.rs` | nur Modulpfade |
| `ansicht/widgets.rs` | `src/bin/gnss-gui/widgets.rs` | nur Modulpfade |
| `ansicht/view_wizard.rs` | `src/bin/gnss-gui/view_wizard.rs` (übernommen 27.11.2026) | letzter Schritt „Töne und Abschluss“ statt „Darstellung und Abschluss“, ohne Update-Abfrage |

Nicht übernommen: `main.rs`, `menu.rs`, `session.rs` (Kommandozeile), `update.rs`, `view_update.rs` (Update), `bin/gnss-gui/main.rs`, `tools/`.

Neu in Feldbuch-NT (kein Gegenstück im Original): `einpassung.rs`, `bewegung.rs`, `punkte.rs`.

## Übertragene Änderungen

| Datum | Richtung | Inhalt |
|---|---|---|
| 27.11.2026 | GNSS-NT → Feldbuch-NT | Commit `c66832e` (Zweig `schnelleinrichtung`): Einrichtungsassistent „Schnelleinrichtung“ (`view_wizard.rs`, Hilfe-Menü, Hilfe-Reiter, automatisch beim ersten Start bzw. nach „Nicht übernehmen“), Feldhervorhebung in der Konfiguration (`view_config::caption`/`set_highlight`), `Settings::set_criteria`, Dialoge über `widgets::dialog` oben links im Arbeitsbereich, SAPOS-Button grün/rot (`view_ntrip`), SAPOS-Feld der Statusleiste grün/rot mit Hinweis. Nicht übernommen: maximierter Start bei FullHD (`fit_window`) – das Fenster gehört Feldbuch-NT. |
