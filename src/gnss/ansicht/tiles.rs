//! Zweck:    Kartenkacheln laden (Netz oder Zwischenspeicher) und als Texturen bereitstellen.
//! Hardware: Windows-PC
//! Autor:    Johann Gerner
//!
//! Übernommen aus gnss-vermessung 0.4.0 (src/bin/gnss-gui/tiles.rs), eigenständig gepflegt in Feldbuch-NT.
//!
//! Geladen werden nur sichtbare Kacheln. Bayern: Open Data der Bayerischen Vermessungsverwaltung
//! (Quellenvermerk erforderlich). OpenStreetMap: Nutzungsrichtlinie der Kachelserver beachten.

use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};
use crate::gnss::kern::config::*;
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const MAX_TILE_BYTES: u64 = 2_000_000;

pub struct Layer {
    pub name: &'static str,
    /// Ordnername im Zwischenspeicher
    key: &'static str,
    /// Adresse mit {z}, {x}, {y}; leer = keine Karte
    url: &'static str,
    pub max_zoom: u8,
    pub attribution: &'static str,
}

const BAYERN: &str = "Datenquelle: Bayerische Vermessungsverwaltung – www.geodaten.bayern.de";

pub const LAYERS: [Layer; 5] = [
    Layer {
        name: "Bayern Webkarte",
        key: "by_webkarte",
        url: "https://wmtsod1.bayernwolke.de/wmts/by_webkarte/smerc/{z}/{x}/{y}",
        max_zoom: 19,
        attribution: BAYERN,
    },
    Layer {
        name: "Bayern Luftbild",
        key: "by_dop",
        url: "https://wmtsod1.bayernwolke.de/wmts/by_dop/smerc/{z}/{x}/{y}",
        max_zoom: 19,
        attribution: BAYERN,
    },
    Layer {
        name: "Bayern amtliche Karte",
        key: "by_amtl_karte",
        url: "https://wmtsod1.bayernwolke.de/wmts/by_amtl_karte/smerc/{z}/{x}/{y}",
        max_zoom: 18,
        attribution: BAYERN,
    },
    Layer {
        name: "OpenStreetMap",
        key: "osm",
        url: "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
        max_zoom: 19,
        attribution: "© OpenStreetMap-Mitwirkende",
    },
    Layer {
        name: "ohne Hintergrund",
        key: "",
        url: "",
        max_zoom: 19,
        attribution: "",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileId {
    pub layer: usize,
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

enum Tile {
    Loading,
    Ready(TextureHandle),
    Failed(Instant),
}

type Loaded = (TileId, Result<ColorImage, String>);

#[derive(Default)]
pub struct Tiles {
    tiles: HashMap<TileId, Tile>,
    workers: Option<(Sender<TileId>, Receiver<Loaded>)>,
    pub last_error: Option<String>,
}

fn cache_path(id: TileId) -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(APP_NAME)
        .join("tiles")
        .join(LAYERS[id.layer].key)
        .join(id.z.to_string())
        .join(format!("{}_{}", id.x, id.y))
}

fn decode(bytes: &[u8]) -> Result<ColorImage, String> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    Ok(ColorImage::from_rgba_unmultiplied(
        [img.width() as usize, img.height() as usize],
        img.as_raw(),
    ))
}

fn fetch(agent: &ureq::Agent, id: TileId) -> Result<ColorImage, String> {
    let path = cache_path(id);
    if let Ok(bytes) = std::fs::read(&path)
        && let Ok(img) = decode(&bytes)
    {
        return Ok(img);
    }
    let url = LAYERS[id.layer]
        .url
        .replace("{z}", &id.z.to_string())
        .replace("{x}", &id.x.to_string())
        .replace("{y}", &id.y.to_string());
    let response = agent
        .get(&url)
        .header("User-Agent", TILE_USER_AGENT)
        .call()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_TILE_BYTES)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let img = decode(&bytes)?;
    if let Some(dir) = path.parent() {
        // Zwischenspeicher ist entbehrlich, Schreibfehler stören nicht
        let _ = std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&path, &bytes));
    }
    Ok(img)
}

fn start_workers(ctx: &Context) -> (Sender<TileId>, Receiver<Loaded>) {
    let (request, jobs) = mpsc::channel::<TileId>();
    let (done, results) = mpsc::channel();
    let jobs = Arc::new(Mutex::new(jobs));
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TILE_TIMEOUT))
        .build()
        .into();
    for _ in 0..TILE_WORKERS {
        let (jobs, done, agent, ctx) = (jobs.clone(), done.clone(), agent.clone(), ctx.clone());
        std::thread::spawn(move || {
            loop {
                let job = jobs.lock().map(|j| j.recv());
                let Ok(Ok(id)) = job else { break };
                if done.send((id, fetch(&agent, id))).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
    }
    (request, results)
}

impl Tiles {
    /// Übernimmt fertig geladene Kacheln.
    pub fn poll(&mut self, ctx: &Context) {
        let Some((_, results)) = &self.workers else {
            return;
        };
        for (id, result) in results.try_iter() {
            let tile = match result {
                Ok(img) => {
                    Tile::Ready(ctx.load_texture(format!("{id:?}"), img, TextureOptions::LINEAR))
                }
                Err(e) => {
                    self.last_error = Some(e);
                    Tile::Failed(Instant::now())
                }
            };
            self.tiles.insert(id, tile);
        }
    }

    /// Textur der Kachel; fordert sie bei Bedarf an.
    pub fn get(&mut self, ctx: &Context, id: TileId) -> Option<TextureHandle> {
        if LAYERS[id.layer].url.is_empty() {
            return None;
        }
        match self.tiles.get(&id) {
            Some(Tile::Ready(t)) => return Some(t.clone()),
            Some(Tile::Loading) => return None,
            Some(Tile::Failed(t)) if t.elapsed() < TILE_RETRY => return None,
            _ => {}
        }
        if self.tiles.len() >= TILE_TEXTURE_LIMIT {
            self.tiles.retain(|_, t| matches!(t, Tile::Loading));
        }
        let (request, _) = self.workers.get_or_insert_with(|| start_workers(ctx));
        if request.send(id).is_ok() {
            self.tiles.insert(id, Tile::Loading);
        }
        None
    }
}
