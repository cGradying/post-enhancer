mod presets;
mod split;
mod store;

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Json, Router,
};
use rayon::prelude::*;
use serde::Deserialize;
use std::{
    hash::{BuildHasher, Hasher},
    io::Write,
    sync::Arc,
};
use store::Store;

struct App {
    store: Store,
    password: Option<String>,
    token: String,
}

#[derive(Deserialize, Default)]
struct Options {
    #[serde(default)]
    presets: Vec<String>,
    split: Option<SplitOptions>,
}

#[derive(Deserialize)]
struct SplitOptions {
    cols: u32,
    rw: u32,
    rh: u32,
    pad: String,
    /// Only split photos at least 1.5x wider than tall; the rest just get presets.
    #[serde(default)]
    wide_only: bool,
}

#[tokio::main]
async fn main() {
    let password = std::env::var("APP_PASSWORD").ok().filter(|p| !p.is_empty());
    if password.is_none() {
        eprintln!("APP_PASSWORD not set: running without a login gate");
    }
    let app = Arc::new(App { store: Store::from_env().await, password, token: random_token() });

    let router = Router::new()
        .route("/", get(|| async { Html(include_str!("../static/index.html")) }))
        .route("/api/export", post(export))
        .route("/api/batches", get(list_batches))
        .route("/api/batches/{id}", get(get_batch))
        .route_layer(middleware::from_fn_with_state(app.clone(), require_login))
        .route("/login", get(|| async { Html(LOGIN_PAGE) }).post(login))
        .route("/fonts/{name}", get(font))
        .layer(DefaultBodyLimit::max(200 * 1024 * 1024))
        .with_state(app);

    let port = std::env::var("PORT").unwrap_or("8080".into());
    let host = std::env::var("HOST").unwrap_or("0.0.0.0".into());
    let listener = tokio::net::TcpListener::bind(format!("{host}:{port}")).await.unwrap();
    println!("listening on http://localhost:{port}");
    axum::serve(listener, router).await.unwrap();
}

// ---- login ----

const LOGIN_PAGE: &str = r#"<!doctype html><meta name=viewport content="width=device-width,initial-scale=1"><title>Post Prep</title>
<style>@font-face{font-family:Silkscreen;font-weight:700;src:url(/fonts/silkscreen-700.woff2)}@font-face{font-family:"Space Grotesk";font-weight:400 700;src:url(/fonts/space-grotesk-400.woff2)}
body{font:15px "Space Grotesk",system-ui,sans-serif;display:grid;place-items:center;min-height:100vh;margin:0;background:#14111c;color:#ece8f7}
form{display:grid;gap:12px;width:min(340px,90vw);background:#1b1626;border:1px solid #352c4d;border-radius:10px;padding:22px}
h1{font:700 20px Silkscreen,monospace;margin:0;color:#fff}p{margin:0;color:#ff8fa3}label{display:grid;gap:6px;font-size:12px;color:#9a90b8}
input{font:inherit;color:inherit;background:#120f1b;border:1px solid #352c4d;border-radius:6px;padding:9px 10px}input:focus-visible{outline:2px solid #a66bff;outline-offset:1px}
button{font:700 14px "Space Grotesk",sans-serif;padding:11px;background:#a66bff;color:#12091f;border:0;border-radius:8px;cursor:pointer}button:hover{filter:brightness(1.08)}</style>
<form method=post action=/login><h1>Post Prep</h1><label>Password <input type=password name=password autofocus required></label><button>Enter</button></form>"#;

async fn font(Path(name): Path<String>) -> Response {
    let bytes: &'static [u8] = match name.as_str() {
        "silkscreen-400.woff2" => include_bytes!("../static/fonts/silkscreen-latin-400-normal.woff2"),
        "silkscreen-700.woff2" => include_bytes!("../static/fonts/silkscreen-latin-700-normal.woff2"),
        "space-grotesk-400.woff2" => include_bytes!("../static/fonts/space-grotesk-latin-400-normal.woff2"),
        "space-grotesk-700.woff2" => include_bytes!("../static/fonts/space-grotesk-latin-700-normal.woff2"),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    ([(header::CONTENT_TYPE, "font/woff2"), (header::CACHE_CONTROL, "public, max-age=31536000")], bytes).into_response()
}

#[derive(Deserialize)]
struct LoginForm {
    password: String,
}

async fn login(State(app): State<Arc<App>>, Form(f): Form<LoginForm>) -> Response {
    match &app.password {
        Some(p) if !constant_time_eq(p.as_bytes(), f.password.as_bytes()) => {
            (StatusCode::UNAUTHORIZED, Html(LOGIN_PAGE.replace("<h1>Post Prep</h1>", "<h1>Post Prep</h1><p>Wrong password.</p>"))).into_response()
        }
        _ => {
            let cookie = format!("session={}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=2592000", app.token);
            ([(header::SET_COOKIE, cookie)], Redirect::to("/")).into_response()
        }
    }
}

async fn require_login(State(app): State<Arc<App>>, req: Request, next: Next) -> Response {
    if app.password.is_none() {
        return next.run(req).await;
    }
    let want = format!("session={}", app.token);
    let ok = req
        .headers()
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .any(|c| constant_time_eq(c.trim().as_bytes(), want.as_bytes()));
    match (ok, req.uri().path()) {
        (true, _) => next.run(req).await,
        (false, "/") => Redirect::to("/login").into_response(),
        (false, _) => StatusCode::UNAUTHORIZED.into_response(),
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

// ponytail: token lives in memory, so a restart logs everyone out and deploy.sh pins App Runner to 1 instance. Sign it with the password if you need more instances.
fn random_token() -> String {
    let part = || std::collections::hash_map::RandomState::new().build_hasher().finish();
    format!("{:016x}{:016x}", part(), part())
}

// ---- export ----

async fn export(State(app): State<Arc<App>>, mut form: Multipart) -> Response {
    let mut opts = Options::default();
    let mut files = Vec::new();
    while let Ok(Some(field)) = form.next_field().await {
        match field.name() {
            Some("options") => match field.text().await.map(|t| serde_json::from_str(&t)) {
                Ok(Ok(o)) => opts = o,
                _ => return (StatusCode::BAD_REQUEST, "bad options").into_response(),
            },
            Some("files") => {
                let name = stem(field.file_name().unwrap_or("photo"));
                match field.bytes().await {
                    Ok(b) => files.push((name, b)),
                    Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
                }
            }
            _ => {}
        }
    }
    if files.is_empty() {
        return (StatusCode::BAD_REQUEST, "no files").into_response();
    }

    let zip = match tokio::task::spawn_blocking(move || build_zip(&files, &opts)).await {
        Ok(Ok(z)) => z,
        Ok(Err(e)) => return (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis().to_string();
    if let Err(e) = app.store.put(&id, zip.clone()).await {
        eprintln!("store batch {id}: {e}");
    }
    zip_response(&id, zip)
}

fn build_zip(files: &[(String, axum::body::Bytes)], opts: &Options) -> Result<Vec<u8>, String> {
    let pad = opts.split.as_ref().map(|s| parse_hex(&s.pad)).unwrap_or(image::Rgb([255, 255, 255]));
    let outputs: Vec<Result<Vec<(String, Vec<u8>)>, String>> = files
        .par_iter()
        .map(|(name, bytes)| {
            let src = presets::decode(bytes).map_err(|e| format!("{name}: {e}"))?;
            let icc = src.icc.as_deref();
            let mut out = Vec::new();
            for p in presets::PRESETS.iter().filter(|p| opts.presets.iter().any(|id| id == p.id)) {
                let img = presets::fit(&src.img, &p.size);
                out.push((format!("{}/{name}_{}.jpg", p.folder, p.id), presets::encode_jpeg(&img, icc)));
            }
            let wide = src.img.width() as u64 * 10 >= src.img.height() as u64 * 15;
            if let Some(s) = opts.split.as_ref().filter(|s| wide || !s.wide_only) {
                let panels = split::split(&src.img, s.cols.clamp(1, 20), (s.rw, s.rh), pad);
                let n = panels.len();
                let target = presets::Size::Exact(1080, (1080 * s.rh.max(1) as u64 / s.rw.max(1) as u64) as u32);
                for (i, panel) in panels.iter().enumerate() {
                    let img = presets::fit(panel, &target);
                    out.push((format!("split/{name}_{}of{n}.jpg", i + 1), presets::encode_jpeg(&img, icc)));
                }
            }
            Ok(out)
        })
        .collect();

    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut errors = String::new();
    for result in outputs {
        match result {
            Ok(entries) => {
                for (path, data) in entries {
                    zip.start_file(path, stored).map_err(|e| e.to_string())?;
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

fn stem(file_name: &str) -> String {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    let base = base.rsplit_once('.').map_or(base, |(s, _)| s);
    let clean: String = base.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    if clean.is_empty() { "photo".into() } else { clean }
}

fn parse_hex(s: &str) -> image::Rgb<u8> {
    let s = s.trim_start_matches('#');
    let c = |i: usize| s.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()).unwrap_or(255);
    image::Rgb([c(0), c(2), c(4)])
}

fn zip_response(id: &str, zip: Vec<u8>) -> Response {
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, "application/zip".parse().unwrap());
    h.insert(header::CONTENT_DISPOSITION, format!("attachment; filename=\"post-prep-{id}.zip\"").parse().unwrap());
    h.insert("x-batch-id", id.parse().unwrap());
    (h, Body::from(zip)).into_response()
}

// ---- history ----

async fn list_batches(State(app): State<Arc<App>>) -> Json<Vec<String>> {
    Json(app.store.list().await)
}

async fn get_batch(State(app): State<Arc<App>>, Path(id): Path<String>) -> Response {
    match app.store.get(&id).await {
        Some(zip) => zip_response(&id, zip),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
