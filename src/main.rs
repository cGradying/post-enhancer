mod enhance;
mod jobs;
mod media;
mod split;
mod store;
mod template;
mod upscale;

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Json, Router,
};
use serde::Deserialize;
use std::{
    hash::{BuildHasher, Hasher},
    path::PathBuf,
    sync::{atomic::Ordering, Arc},
};
use store::Store;

struct App {
    /// Served on this machine only, so it may write to folders the user picks.
    local: bool,
    port: String,
    store: Store,
    jobs: jobs::Jobs,
    password: Option<String>,
    token: String,
}

#[tokio::main]
async fn main() {
    let password = std::env::var("APP_PASSWORD").ok().filter(|p| !p.is_empty());
    if password.is_none() {
        eprintln!("APP_PASSWORD not set: running without a login gate");
    }
    let port = std::env::var("PORT").unwrap_or("8080".into());
    let host = std::env::var("HOST").unwrap_or("0.0.0.0".into());
    let local = matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1");
    let app = Arc::new(App { local, port: port.clone(), store: Store::from_env().await, jobs: Default::default(), password, token: random_token() });
    let max_mb: usize = std::env::var("MAX_UPLOAD_MB").ok().and_then(|v| v.parse().ok()).unwrap_or(1024);

    let router = Router::new()
        .route("/", get(|| async { Html(include_str!("../static/index.html")) }))
        .route("/app.css", get(|| async { ([(header::CONTENT_TYPE, "text/css")], include_str!("../static/app.css")) }))
        .route("/app.js", get(|| async { ([(header::CONTENT_TYPE, "text/javascript")], include_str!("../static/app.js")) }))
        .route("/template.js", get(|| async { ([(header::CONTENT_TYPE, "text/javascript")], include_str!("../static/template.js")) }))
        .route("/api/jobs", post(create_job))
        .route("/api/jobs/{id}", get(job_status))
        .route("/api/preview", post(preview))
        .route("/api/capabilities", get(capabilities))
        .route("/api/choose-folder", post(choose_folder))
        .route("/api/reveal", post(reveal))
        .route("/api/batches", get(list_batches))
        .route("/api/batches/{id}", get(get_batch))
        .route_layer(middleware::from_fn_with_state(app.clone(), require_login))
        .route("/login", get(|| async { Html(LOGIN_PAGE) }).post(login))
        .route("/fonts/{name}", get(font))
        .layer(DefaultBodyLimit::max(max_mb * 1024 * 1024))
        .with_state(app);

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

// ---- jobs ----

async fn create_job(State(app): State<Arc<App>>, headers: HeaderMap, mut form: Multipart) -> Response {
    let mut opts = None;
    let mut files = Vec::new();
    let mut layers = jobs::Layers { under: None, over: None };
    loop {
        let field = match form.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return upload_error(&e),
        };
        let field_name = field.name().unwrap_or("").to_string();
        let file_name = field.file_name().unwrap_or("photo").to_string();
        let bytes = match field.bytes().await {
            Ok(b) => b,
            Err(e) => return upload_error(&e),
        };
        match field_name.as_str() {
            "options" => match serde_json::from_slice::<jobs::Options>(&bytes) {
                Ok(o) => opts = Some(o),
                Err(e) => return (StatusCode::BAD_REQUEST, format!("bad options: {e}")).into_response(),
            },
            "files" => files.push(jobs::Upload { name: file_name, bytes }),
            "under" | "over" => {
                let layer = match image::load_from_memory(&bytes) {
                    Ok(i) => i.to_rgba8(),
                    Err(e) => return (StatusCode::BAD_REQUEST, format!("bad {field_name} layer: {e}")).into_response(),
                };
                if field_name == "under" { layers.under = Some(layer) } else { layers.over = Some(layer) }
            }
            _ => {}
        }
    }
    let Some(opts) = opts else { return (StatusCode::BAD_REQUEST, "missing options").into_response() };
    if files.is_empty() {
        return (StatusCode::BAD_REQUEST, "no files").into_response();
    }

    let dir = match opts.out_dir.as_deref() {
        None => None,
        Some(base) => match local_request(&app, &headers).and_then(|()| export_dir(base, opts.out_name.as_deref())) {
            Ok(d) => Some(d),
            Err(e) => return (StatusCode::FORBIDDEN, e).into_response(),
        },
    };

    let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis().to_string();
    let job = app.jobs.create(&id, files.len());
    let (app2, id2) = (app.clone(), id.clone());
    tokio::spawn(async move {
        let worker = job.clone();
        let result = tokio::task::spawn_blocking(move || jobs::run(&files, &opts, &layers, &worker, dir.as_deref())).await;
        let state = match result {
            Ok(Ok(jobs::Output::Zip(zip))) => match app2.store.put(&id2, zip).await {
                Ok(()) => jobs::State::Done { batch_id: id2 },
                Err(e) => jobs::State::Failed { error: format!("couldn't save the batch: {e}") },
            },
            Ok(Ok(jobs::Output::Saved { folder, files })) => jobs::State::Saved { folder, files },
            Ok(Err(e)) => jobs::State::Failed { error: e },
            Err(e) => jobs::State::Failed { error: e.to_string() },
        };
        job.finish(state);
    });
    Json(serde_json::json!({ "id": id })).into_response()
}

async fn job_status(State(app): State<Arc<App>>, Path(id): Path<String>) -> Response {
    let Some(job) = app.jobs.get(&id) else { return StatusCode::NOT_FOUND.into_response() };
    let state = job.state.lock().unwrap().clone();
    let mut v = serde_json::to_value(state).unwrap();
    v["done"] = job.done.load(Ordering::Relaxed).into();
    v["total"] = job.total.into();
    Json(v).into_response()
}

/// A browser-viewable JPEG (≤1600 px) for formats browsers can't show: HEIC, TIFF, RAW.
async fn preview(mut form: Multipart) -> Response {
    let Ok(Some(field)) = form.next_field().await else { return StatusCode::BAD_REQUEST.into_response() };
    let name = field.file_name().unwrap_or("photo").to_string();
    let Ok(bytes) = field.bytes().await else { return StatusCode::BAD_REQUEST.into_response() };
    let result = tokio::task::spawn_blocking(move || {
        let src = media::decode(&bytes, &name)?;
        let (w, h) = src.img.dimensions();
        let s = (1600.0 / w.max(h) as f64).min(1.0);
        let small = media::resize(&src.img, ((w as f64 * s) as u32).max(1), ((h as f64 * s) as u32).max(1));
        let jpg = media::encode(&small, media::Format::Jpeg, 85, src.icc.as_deref(), &media::Meta::default());
        Ok::<_, String>((jpg, w, h))
    })
    .await;
    match result {
        Ok(Ok((jpg, w, h))) => ([(header::CONTENT_TYPE, "image/jpeg".to_string()), (header::HeaderName::from_static("x-source-size"), format!("{w}x{h}"))], jpg).into_response(),
        Ok(Err(e)) => (StatusCode::UNPROCESSABLE_ENTITY, e).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn capabilities(State(app): State<Arc<App>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ai_upscale": upscale::ai_bin().is_some(),
        "heic": cfg!(target_os = "macos"),
        "version": env!("CARGO_PKG_VERSION"),
        "local": app.local,
        "can_choose": app.local && cfg!(target_os = "macos"),
        "default_dir": app.local.then(|| home().join("Downloads").join("Post Prep").display().to_string()),
    }))
}

// ---- saving to a folder on this machine ----

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into()))
}

fn upload_error(e: &axum::extract::multipart::MultipartError) -> Response {
    (e.status(), e.body_text()).into_response()
}

/// Folder writes only come from this app's own page. A web page on the internet can post to
/// localhost too, but cannot send the custom header or fake the Host header.
fn local_request(app: &App, headers: &HeaderMap) -> Result<(), String> {
    if !app.local {
        return Err("saving to a folder only works in the Mac app".into());
    }
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
    let ok_host = [format!("localhost:{}", app.port), format!("127.0.0.1:{}", app.port)].iter().any(|h| h == host);
    let ok_origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()).is_none_or(|o| o == format!("http://{host}"));
    if ok_host && ok_origin && headers.get("x-post-prep").is_some_and(|v| v == "1") {
        Ok(())
    } else {
        Err("this request did not come from the Post Prep page".into())
    }
}

/// `base` must be an absolute folder (a leading ~ is allowed) inside the home folder or on an external drive.
fn user_folder(base: &str) -> Result<PathBuf, String> {
    let base = base.trim();
    let p = match base.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None if base == "~" => home(),
        None => PathBuf::from(base),
    };
    if !p.is_absolute() || p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("choose a full folder path, like ~/Downloads/Post Prep".into());
    }
    if !(p.starts_with(home()) || p.starts_with("/Volumes")) {
        return Err("the folder must be inside your home folder or on an external drive".into());
    }
    Ok(p)
}

fn export_dir(base: &str, name: Option<&str>) -> Result<PathBuf, String> {
    let dir = user_folder(base)?;
    Ok(match name {
        Some(n) => dir.join(jobs::dir_name(n)),
        None => dir,
    })
}

/// Native folder picker. The browser cannot hand over a real path, so the Mac app asks macOS.
async fn choose_folder(State(app): State<Arc<App>>, headers: HeaderMap) -> Response {
    if let Err(e) = local_request(&app, &headers) {
        return (StatusCode::FORBIDDEN, e).into_response();
    }
    let run = tokio::process::Command::new("osascript")
        .args(["-e", "tell current application to activate", "-e", "POSIX path of (choose folder with prompt \"Save exports to\")"])
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(std::time::Duration::from_secs(300), run).await {
        Ok(Ok(out)) if out.status.success() => {
            let path = String::from_utf8_lossy(&out.stdout).trim().trim_end_matches('/').to_string();
            Json(serde_json::json!({ "path": path })).into_response()
        }
        Ok(Ok(_)) => Json(serde_json::json!({ "cancelled": true })).into_response(),
        Ok(Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, format!("couldn't open the folder picker: {e}")).into_response(),
        Err(_) => (StatusCode::REQUEST_TIMEOUT, "the folder picker timed out").into_response(),
    }
}

#[derive(Deserialize)]
struct RevealRequest {
    path: String,
}

async fn reveal(State(app): State<Arc<App>>, headers: HeaderMap, Json(r): Json<RevealRequest>) -> Response {
    let path = match local_request(&app, &headers).and_then(|()| user_folder(&r.path)) {
        Ok(p) if p.is_dir() => p,
        Ok(_) => return (StatusCode::NOT_FOUND, "that folder does not exist").into_response(),
        Err(e) => return (StatusCode::FORBIDDEN, e).into_response(),
    };
    match std::process::Command::new("open").arg(path).status() {
        Ok(s) if s.success() => StatusCode::NO_CONTENT.into_response(),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "couldn't open the folder").into_response(),
    }
}

fn zip_response(id: &str, zip: Vec<u8>) -> Response {
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, "application/zip".parse().unwrap());
    h.insert(header::CONTENT_DISPOSITION, format!("attachment; filename=\"post-prep-{id}.zip\"").parse().unwrap());
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
