use crate::media::{self, Format, Meta, Size};
use crate::upscale::{self, Mode};
use crate::{enhance, split, template};
use axum::body::Bytes;
use image::RgbaImage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Deserialize, Default, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    #[default]
    Post,
    Template,
}

#[derive(Deserialize)]
pub struct Options {
    #[serde(default)]
    pub tool: Tool,
    #[serde(default)]
    pub presets: Vec<String>,
    pub split: Option<SplitOptions>,
    /// Extra output: the whole photo upscaled ×2 or ×4.
    pub originals: Option<u32>,
    pub layout: Option<template::Layout>,
    #[serde(default)]
    pub format: Format,
    #[serde(default = "q95")]
    pub quality: u8,
    #[serde(default)]
    pub upscale: Mode,
    #[serde(default)]
    pub enhance: bool,
    #[serde(default)]
    pub rename: String,
    #[serde(default)]
    pub event: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub meta: Meta,
}

fn q95() -> u8 {
    95
}

#[derive(Deserialize)]
pub struct SplitOptions {
    pub cols: u32,
    pub rw: u32,
    pub rh: u32,
    pub pad: String,
    /// Only split photos at least 1.5x wider than tall; the rest just get presets.
    #[serde(default)]
    pub wide_only: bool,
}

pub struct Upload {
    pub name: String,
    pub bytes: Bytes,
}

pub struct Layers {
    pub under: Option<RgbaImage>,
    pub over: Option<RgbaImage>,
}

// ---- progress registry ----

#[derive(Serialize, Clone)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum State {
    Running,
    Done { batch_id: String },
    Failed { error: String },
}

pub struct Job {
    pub total: usize,
    pub done: AtomicUsize,
    pub state: Mutex<State>,
    finished: Mutex<Option<Instant>>,
}

impl Job {
    pub fn finish(&self, state: State) {
        *self.state.lock().unwrap() = state;
        *self.finished.lock().unwrap() = Some(Instant::now());
    }
}

#[derive(Default)]
pub struct Jobs(Mutex<HashMap<String, Arc<Job>>>);

impl Jobs {
    pub fn create(&self, id: &str, total: usize) -> Arc<Job> {
        let job = Arc::new(Job { total, done: AtomicUsize::new(0), state: Mutex::new(State::Running), finished: Mutex::new(None) });
        let mut map = self.0.lock().unwrap();
        map.retain(|_, j| j.finished.lock().unwrap().is_none_or(|t| t.elapsed() < Duration::from_secs(600)));
        map.insert(id.to_string(), job.clone());
        job
    }

    pub fn get(&self, id: &str) -> Option<Arc<Job>> {
        self.0.lock().unwrap().get(id).cloned()
    }
}

// ---- processing ----

/// Process every upload in parallel and pack the results into one zip.
/// Per-file failures go to errors.txt instead of failing the batch.
pub fn run(files: &[Upload], opts: &Options, layers: &Layers, job: &Job) -> Result<Vec<u8>, String> {
    let outputs: Vec<Result<Vec<(String, Vec<u8>)>, String>> = files
        .par_iter()
        .enumerate()
        .map(|(i, f)| {
            let r = process(f, i + 1, opts, layers);
            job.done.fetch_add(1, Ordering::Relaxed);
            r
        })
        .collect();

    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut used = HashSet::new();
    let mut errors = String::new();
    for result in outputs {
        match result {
            Ok(entries) => {
                for (path, data) in entries {
                    zip.start_file(unique(&mut used, path), stored).map_err(|e| e.to_string())?;
                    zip.write_all(&data).map_err(|e| e.to_string())?;
                }
            }
            Err(e) => errors.push_str(&format!("{e}\n")),
        }
    }
    if !errors.is_empty() {
        zip.start_file("errors.txt", stored).map_err(|e| e.to_string())?;
        zip.write_all(errors.as_bytes()).map_err(|e| e.to_string())?;
    }
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}

fn process(f: &Upload, n: usize, opts: &Options, layers: &Layers) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut src = media::decode(&f.bytes, &f.name).map_err(|e| format!("{}: {e}", f.name))?;
    if opts.enhance {
        enhance::auto(&mut src.img);
    }
    let stem = stem(&f.name);
    let ext = opts.format.ext();
    let encode = |img: &image::RgbImage| media::encode(img, opts.format, opts.quality, src.icc.as_deref(), &opts.meta);
    let name = |preset: &str| rename(&opts.rename, &stem, &opts.event, &opts.date, n, preset);
    let mut out = Vec::new();

    match opts.tool {
        Tool::Template => {
            let layout = opts.layout.as_ref().ok_or("template layout missing")?;
            let img = template::compose(&src.img, layout, layers.under.as_ref(), layers.over.as_ref(), opts.upscale);
            out.push((format!("template/{}.{ext}", name("framed")), encode(&img)));
        }
        Tool::Post => {
            for p in media::PRESETS.iter().filter(|p| opts.presets.iter().any(|id| id == p.id)) {
                let img = media::fit(&src.img, &p.size, opts.upscale);
                out.push((format!("{}/{}.{ext}", p.folder, name(p.id)), encode(&img)));
            }
            let wide = src.img.width() as u64 * 10 >= src.img.height() as u64 * 15;
            if let Some(s) = opts.split.as_ref().filter(|s| wide || !s.wide_only) {
                let panels = split::split(&src.img, s.cols.clamp(1, 20), (s.rw, s.rh), template::parse_hex(&s.pad));
                let total = panels.len();
                let target = Size::Exact(1080, (1080 * s.rh.max(1) as u64 / s.rw.max(1) as u64) as u32);
                for (i, panel) in panels.iter().enumerate() {
                    let img = media::fit(panel, &target, opts.upscale);
                    out.push((format!("split/{}.{ext}", name(&format!("{}of{total}", i + 1))), encode(&img)));
                }
            }
            if let Some(factor) = opts.originals {
                let f = factor.clamp(2, 4);
                let img = upscale::by_factor(&src.img, f, opts.upscale);
                out.push((format!("upscaled/{}.{ext}", name(&format!("x{f}"))), encode(&img)));
            }
        }
    }
    Ok(out)
}

/// Fill a rename pattern. Tokens: {name} {event} {date} {n} {n:03} {preset}.
/// An empty pattern means "{name}_{preset}".
pub fn rename(pattern: &str, name: &str, event: &str, date: &str, n: usize, preset: &str) -> String {
    let pattern = if pattern.trim().is_empty() { "{name}_{preset}" } else { pattern };
    let mut out = pattern.replace("{name}", name).replace("{event}", event).replace("{date}", date).replace("{preset}", preset);
    while let Some(start) = out.find("{n") {
        let Some(len) = out[start..].find('}') else { break };
        let width = out[start + 2..start + len].trim_start_matches(':').parse::<usize>().unwrap_or(0);
        out.replace_range(start..=start + len, &format!("{n:0width$}"));
    }
    let clean = sanitize(&out);
    if clean.is_empty() { "photo".into() } else { clean }
}

fn sanitize(s: &str) -> String {
    let s: String = s.chars().map(|c| if c.is_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
    s.trim_matches(['_', '.']).to_string()
}

pub fn stem(file_name: &str) -> String {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    sanitize(base.rsplit_once('.').map_or(base, |(s, _)| s))
}

fn unique(used: &mut HashSet<String>, path: String) -> String {
    if used.insert(path.clone()) {
        return path;
    }
    let (base, ext) = path.rsplit_once('.').unwrap_or((&path, ""));
    (2..).map(|i| format!("{base}-{i}.{ext}")).find(|p| used.insert(p.clone())).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_tokens() {
        assert_eq!(rename("{event}_{n:03}", "IMG_1", "shoot", "", 1, "framed"), "shoot_001");
        assert_eq!(rename("", "IMG 1", "", "", 7, "ig_tall"), "IMG_1_ig_tall");
        assert_eq!(rename("{date}-{name}-{n}", "a", "", "2026-10-23", 12, "x"), "2026-10-23-a-12");
        assert_eq!(rename("../../{name}", "a", "", "", 1, "x"), "a");
    }

    #[test]
    fn duplicate_paths_get_suffixes() {
        let mut used = HashSet::new();
        assert_eq!(unique(&mut used, "t/a.jpg".into()), "t/a.jpg");
        assert_eq!(unique(&mut used, "t/a.jpg".into()), "t/a-2.jpg");
        assert_eq!(unique(&mut used, "t/a.jpg".into()), "t/a-3.jpg");
    }
}
