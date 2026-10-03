//! Game library service (`gb-web`) — GEP §7, Appendix D.
//!
//! An HTTP/1.1 server on `127.0.0.1` that hosts a game library, its JSON API
//! (Appendix D), the embedded web front end (Appendix E) and the WebAssembly
//! module. Everything — HTTP, JSON, SHA-256, PNG and multipart parsing — is
//! hand-written on `std`, because the workspace takes no external crates
//! (R-BASE-1).
//!
//! One thread per connection; a panic while handling a request is caught and
//! answered with `500 emulation_failed` so a single bad request never takes
//! the server down (R-BASE-6).

// Some functions in this crate are stubs (`todo!()`, see their doc comments); the
// helpers they used are still here, so they show up as unused until the stubs are
// implemented again. Remove this allow when they are.
#![allow(dead_code, unused_imports)]

mod http;
mod json;
mod png;
mod sha256;
mod store;

use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gb_core::{Emulator, Model, SCREEN_HEIGHT, SCREEN_WIDTH};

use http::{ParseError, Request, Response};
use store::Game;

const INDEX_HTML: &str = include_str!("../static/index.html");
const PLAYER_HTML: &str = include_str!("../static/player.html");
/// Where `--wasm` looks by default (R-WEB-1).
const DEFAULT_WASM: &str = "target/wasm32-unknown-unknown/release/gb_wasm.wasm";
/// How many rendered screenshots to keep.
const SCREENSHOT_CACHE: usize = 64;
/// A screenshot render failing to finish within this is `500 emulation_failed`.
const RENDER_BUDGET: Duration = Duration::from_secs(30);
const DEFAULT_FRAMES: u32 = 300;
const MAX_FRAMES: u32 = 3600;

type CacheKey = (String, u32, u8);
type ScreenshotCache = Mutex<HashMap<CacheKey, Vec<u8>>>;

/// Command-line configuration (R-WEB-1).
struct Config {
    port: u16,
    library: PathBuf,
    seeds: Vec<PathBuf>,
    wasm: PathBuf,
}

/// Counters since the process started (Appendix D.8).
#[derive(Default)]
struct Stats {
    uploads_accepted: u64,
    uploads_rejected: u64,
    screenshots_rendered: u64,
    emulated_frames: u64,
}

/// Shared server state.
struct App {
    store: store::Store,
    wasm: Option<Vec<u8>>,
    stats: Mutex<Stats>,
    cache: ScreenshotCache,
    /// Serialises the duplicate check and insert so concurrent uploads of the
    /// same ROM store exactly one game (R-WEB-10).
    upload_lock: Mutex<()>,
    start: Instant,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = match parse_args(&args) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("gb-web: {message}");
            eprintln!("usage: gb-web --port P --library DIR [--seed DIR]... [--wasm FILE]");
            std::process::exit(2);
        }
    };
    if let Err(error) = run(config) {
        eprintln!("gb-web: {error}");
        std::process::exit(1);
    }
}

fn run(config: Config) -> std::io::Result<()> {
    // OI-6: seed directories are imported only when the library is created —
    // that is, when the directory is absent or empty. Check before `Store::open`
    // creates its subdirectories.
    let fresh = library_is_new(&config.library);
    let store = store::Store::open(&config.library)?;
    if fresh {
        import_seeds(&config.seeds, &store);
    }

    let wasm = std::fs::read(&config.wasm).ok();
    if wasm.is_none() {
        eprintln!(
            "gb-web: warning: wasm module {} not found; /gb_wasm.wasm will 404",
            config.wasm.display()
        );
    }
    let app = Arc::new(App {
        store,
        wasm,
        stats: Mutex::new(Stats::default()),
        cache: Mutex::new(HashMap::new()),
        upload_lock: Mutex::new(()),
        start: Instant::now(),
    });

    let listener = TcpListener::bind(("127.0.0.1", config.port))?;
    eprintln!(
        "gb-web: listening on http://127.0.0.1:{} (library {})",
        config.port,
        config.library.display()
    );
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let app = Arc::clone(&app);
                thread::spawn(move || serve_conn(app, stream));
            }
            Err(error) => eprintln!("gb-web: accept error: {error}"),
        }
    }
    Ok(())
}

/// Whether the library directory is absent or has no entries.
fn library_is_new(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => true,
    }
}

/// Import every valid, playable `*.gb`/`*.gbc` from the seed directories.
fn import_seeds(seeds: &[PathBuf], store: &store::Store) {
    for dir in seeds {
        let Ok(entries) = std::fs::read_dir(dir) else {
            eprintln!("gb-web: cannot read seed directory {}", dir.display());
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let extension = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase());
            if !matches!(extension.as_deref(), Some("gb" | "gbc")) {
                continue;
            }
            let Ok(rom) = std::fs::read(&path) else {
                continue;
            };
            if store::validate_rom(&rom).is_err() {
                continue;
            }
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("seed.gb")
                .to_string();
            let game = Game::from_rom(&rom, &filename, unix_now());
            if !game.playable || store.has(&game.id) {
                continue;
            }
            if let Err(error) = store.insert(&game, &rom) {
                eprintln!("gb-web: could not import {}: {error}", path.display());
            }
        }
    }
}

fn serve_conn(app: Arc<App>, mut stream: TcpStream) {
    let result = catch_unwind(AssertUnwindSafe(|| handle_conn(&app, &mut stream)));
    if result.is_err() {
        let _ = Response::error(
            500,
            "the emulator failed while handling the request",
            "emulation_failed",
        )
        .write_to(&mut stream);
    }
}

/// Read one request, answer it, and return; the response closes the connection.
fn handle_conn(app: &App, stream: &mut TcpStream) -> std::io::Result<()> {
    match http::read_request(stream) {
        Ok(Some(request)) => handle(app, request).write_to(stream),
        Ok(None) => Ok(()),
        Err(ParseError::TooLarge) => {
            Response::error(400, "request header too large", "bad_request").write_to(stream)
        }
        Err(_) => Ok(()),
    }
}

fn handle(app: &App, request: Request) -> Response {
    let owned_path = request.path.clone();
    let path = owned_path.as_str();
    if path == "/" {
        return if request.method == "GET" {
            Response::html(200, INDEX_HTML)
        } else {
            method_not_allowed()
        };
    }
    if path == "/gb_wasm.wasm" {
        return if request.method == "GET" {
            match &app.wasm {
                Some(bytes) => Response::new(200, "application/wasm", bytes.clone()),
                None => Response::error(404, "wasm module not available", "not_found"),
            }
        } else {
            method_not_allowed()
        };
    }
    if let Some(rest) = path.strip_prefix("/play/") {
        return play_page(app, rest, &request.method);
    }
    if path.starts_with("/static/") || path == "/favicon.ico" {
        return not_found_html();
    }
    if let Some(rest) = path.strip_prefix("/api") {
        return api(app, rest, request);
    }
    not_found_html()
}

fn play_page(app: &App, id: &str, method: &str) -> Response {
    if method != "GET" {
        return method_not_allowed();
    }
    if id.contains('/') || !valid_id(id) || find_game(app, id).is_none() {
        return not_found_html();
    }
    Response::html(200, PLAYER_HTML)
}

fn api(app: &App, rest: &str, request: Request) -> Response {
    let segments: Vec<&str> = rest
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    match segments.as_slice() {
        ["health"] => api_health(app, &request.method),
        ["stats"] => api_stats(app, &request.method),
        ["games"] => api_games(app, request),
        ["games", id] => api_game(app, id, request),
        ["games", id, "rom"] => api_rom(app, id, &request.method),
        ["games", id, "screenshot.png"] => api_screenshot(app, id, request),
        ["games", id, "save"] => api_save(app, id, request),
        _ => Response::error(404, "no such endpoint", "not_found"),
    }
}

fn api_health(app: &App, method: &str) -> Response {
    if method != "GET" {
        return method_not_allowed();
    }
    Response::json(
        200,
        format!("{{\"ok\":true,\"games\":{}}}", app.store.games().len()),
    )
}

fn api_stats(app: &App, method: &str) -> Response {
    if method != "GET" {
        return method_not_allowed();
    }
    let stats = lock(&app.stats);
    let body = format!(
        "{{\"games\":{},\"uploads_accepted\":{},\"uploads_rejected\":{},\"screenshots_rendered\":{},\"emulated_frames\":{},\"uptime_sec\":{}}}",
        app.store.games().len(),
        stats.uploads_accepted,
        stats.uploads_rejected,
        stats.screenshots_rendered,
        stats.emulated_frames,
        app.start.elapsed().as_secs(),
    );
    Response::json(200, body)
}

fn api_games(app: &App, request: Request) -> Response {
    match request.method.as_str() {
        "GET" => api_list(app, &request),
        "POST" => api_upload(app, request),
        _ => method_not_allowed(),
    }
}

/// A validated `/api/games` query (Appendix D.3).
#[derive(Default)]
struct ListQuery {
    q: Option<String>,
    cgb: Option<String>,
    mapper: Option<String>,
    playable: Option<bool>,
    sort: Option<String>,
    order: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

#[allow(unused_variables)]
fn api_list(app: &App, request: &Request) -> Response {
    todo!("GET /api/games: filters, search, sort, order, paging (Appendix D.3)")
}

fn game_matches(game: &Game, query: &ListQuery) -> bool {
    if let Some(cgb) = &query.cgb {
        if game.cgb != cgb {
            return false;
        }
    }
    if let Some(mapper) = &query.mapper {
        if !game.mapper.eq_ignore_ascii_case(mapper) {
            return false;
        }
    }
    if let Some(playable) = query.playable {
        if game.playable != playable {
            return false;
        }
    }
    if let Some(term) = &query.q {
        let needle = term.to_lowercase();
        if !game.title.to_lowercase().contains(&needle)
            && !game.filename.to_lowercase().contains(&needle)
        {
            return false;
        }
    }
    true
}

fn sort_games(games: &mut [Game], query: &ListQuery) {
    let descending = query.order.as_deref() == Some("desc");
    let key = query.sort.as_deref().unwrap_or("title");
    games.sort_by(|a, b| {
        let primary = match key {
            "added" => a.added.cmp(&b.added),
            "size" => a.size.cmp(&b.size),
            _ => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
        };
        let primary = if descending {
            primary.reverse()
        } else {
            primary
        };
        // Ties break by id ascending whatever the order (D.3).
        primary.then_with(|| a.id.cmp(&b.id))
    });
}

fn api_game(app: &App, id: &str, request: Request) -> Response {
    if !valid_id(id) {
        return Response::error(404, "unknown game", "not_found");
    }
    match request.method.as_str() {
        "GET" => match find_game(app, id) {
            Some(game) => Response::json(200, game.to_json()),
            None => Response::error(404, "unknown game", "not_found"),
        },
        "DELETE" => {
            if !app.store.has(id) {
                return Response::error(404, "unknown game", "not_found");
            }
            match app.store.delete(id) {
                Ok(()) => Response::empty(204),
                Err(error) => {
                    eprintln!("gb-web: delete failed: {error}");
                    Response::error(500, "could not delete the game", "emulation_failed")
                }
            }
        }
        _ => method_not_allowed(),
    }
}

fn api_rom(app: &App, id: &str, method: &str) -> Response {
    if method != "GET" {
        return method_not_allowed();
    }
    if !valid_id(id) {
        return Response::error(404, "unknown game", "not_found");
    }
    let Some(game) = find_game(app, id) else {
        return Response::error(404, "unknown game", "not_found");
    };
    let Some(rom) = app.store.rom(id) else {
        return Response::error(404, "unknown game", "not_found");
    };
    Response::new(200, "application/octet-stream", rom).with_header(
        "Content-Disposition",
        format!(
            "attachment; filename=\"{}\"",
            attachment_name(&game.filename)
        ),
    )
}

#[allow(unused_variables)]
fn api_screenshot(app: &App, id: &str, request: Request) -> Response {
    todo!("GET /api/games/{id}/screenshot.png: parameters, caching, errors (Appendix D.6)")
}

#[allow(unused_variables)]
fn render_screenshot(app: &App, id: &str, frames: u32, model: Model) -> Result<Vec<u8>, Response> {
    todo!("run the game headless for `frames` frames and encode the frame as PNG (Appendix D.6)")
}

fn api_save(app: &App, id: &str, request: Request) -> Response {
    if !valid_id(id) {
        return Response::error(404, "unknown game", "not_found");
    }
    let Some(game) = find_game(app, id) else {
        return Response::error(404, "unknown game", "not_found");
    };
    match request.method.as_str() {
        "GET" => match app.store.save(id) {
            Some(data) => Response::new(200, "application/octet-stream", data),
            None => Response::error(404, "no save stored for this game", "not_found"),
        },
        "PUT" => {
            if !game.battery {
                return bad_request("this cartridge has no battery save");
            }
            if request.body_truncated || request.body.len() < game.ram_size {
                return bad_request("save body must be exactly `ram_size` bytes");
            }
            match app.store.put_save(id, &request.body) {
                Ok(()) => Response::empty(204),
                Err(error) => {
                    eprintln!("gb-web: save failed: {error}");
                    Response::error(500, "could not store the save", "emulation_failed")
                }
            }
        }
        "DELETE" => match app.store.delete_save(id) {
            Ok(true) => Response::empty(204),
            Ok(false) => Response::error(404, "no save stored for this game", "not_found"),
            Err(error) => {
                eprintln!("gb-web: save delete failed: {error}");
                Response::error(500, "could not delete the save", "emulation_failed")
            }
        },
        _ => method_not_allowed(),
    }
}

#[allow(unused_variables)]
fn api_upload(app: &App, request: Request) -> Response {
    todo!("POST /api/games: multipart or raw upload, validation and the product owner's upload rules (Appendix D.4)")
}

/// Count a 4xx upload answer in the statistics and return it unchanged.
fn upload_result(app: &App, response: Response) -> Response {
    if (400..500).contains(&response.status) {
        lock(&app.stats).uploads_rejected += 1;
    }
    response
}

fn rom_error_message(error: store::RomError) -> &'static str {
    match error {
        store::RomError::Size => "not a Game Boy ROM: bad size",
        store::RomError::Logo => "not a Game Boy ROM: bad Nintendo logo",
        store::RomError::Checksum => "not a Game Boy ROM: bad header checksum",
    }
}

fn find_game(app: &App, id: &str) -> Option<Game> {
    app.store.games().into_iter().find(|game| game.id == id)
}

fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_mapper_name(value: &str) -> bool {
    const NAMES: [&str; 13] = [
        "ROM", "MBC1", "MBC2", "MMM01", "MBC3", "MBC5", "MBC6", "MBC7", "CAMERA", "TAMA5", "HUC3",
        "HUC1", "UNKNOWN",
    ];
    NAMES.iter().any(|name| name.eq_ignore_ascii_case(value))
}

/// Make a file name safe for an `attachment` header value.
fn attachment_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c == '"' || c == '\\' || c.is_control() {
            out.push('_');
        } else {
            out.push(c);
        }
    }
    if out.is_empty() {
        "rom.gb".to_string()
    } else {
        out
    }
}

fn method_not_allowed() -> Response {
    Response::error(405, "method not allowed", "method_not_allowed")
}

fn bad_request(message: &str) -> Response {
    Response::error(400, message, "bad_request")
}

fn not_found_html() -> Response {
    Response::html(
        404,
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Not found</title></head>\
         <body><h1>404 — not found</h1></body></html>",
    )
}

// ---------------------------------------------------------------------------
// Multipart parsing
// ---------------------------------------------------------------------------

/// One part of a `multipart/form-data` body.
#[derive(Debug)]
struct Part {
    name: String,
    filename: Option<String>,
    data: Vec<u8>,
}

/// The `boundary` parameter of a `multipart/form-data` content type.
fn multipart_boundary(content_type: &str) -> Option<String> {
    let index = content_type.to_ascii_lowercase().find("boundary=")?;
    let rest = &content_type[index + "boundary=".len()..];
    let rest = rest.split(';').next().unwrap_or(rest).trim();
    let boundary = unquote(rest);
    if boundary.is_empty() {
        None
    } else {
        Some(boundary.to_string())
    }
}

/// Split a `multipart/form-data` body into its parts.
fn parse_multipart(body: &[u8], boundary: &str) -> Vec<Part> {
    let mut delimiter = Vec::with_capacity(boundary.len() + 2);
    delimiter.extend_from_slice(b"--");
    delimiter.extend_from_slice(boundary.as_bytes());
    let positions = find_all(body, &delimiter);

    let mut parts = Vec::new();
    for (index, &position) in positions.iter().enumerate() {
        let start = position + delimiter.len();
        let end = positions.get(index + 1).copied().unwrap_or(body.len());
        if start >= end {
            continue;
        }
        let mut segment = &body[start..end];
        // The final delimiter is followed by `--`.
        if segment.starts_with(b"--") {
            break;
        }
        if let Some(rest) = segment.strip_prefix(b"\r\n") {
            segment = rest;
        } else if let Some(rest) = segment.strip_prefix(b"\n") {
            segment = rest;
        }

        let (header_end, separator_len) = match http::find(segment, b"\r\n\r\n") {
            Some(position) => (position, 4),
            None => match http::find(segment, b"\n\n") {
                Some(position) => (position, 2),
                None => continue,
            },
        };

        let header_text = String::from_utf8_lossy(&segment[..header_end]);
        let mut name = None;
        let mut filename = None;
        for line in header_text.split('\n') {
            let line = line.trim_end_matches('\r');
            if line
                .to_ascii_lowercase()
                .starts_with("content-disposition:")
            {
                name = disposition_value(line, "name").or(name);
                filename = disposition_value(line, "filename").or(filename);
            }
        }
        let Some(name) = name else {
            continue;
        };

        let mut data = &segment[header_end + separator_len..];
        if let Some(rest) = data.strip_suffix(b"\r\n") {
            data = rest;
        } else if let Some(rest) = data.strip_suffix(b"\n") {
            data = rest;
        }
        parts.push(Part {
            name,
            filename,
            data: data.to_vec(),
        });
    }
    parts
}

/// The value of a `key=...` parameter within one `;`-separated disposition
/// segment. Checking whole segments avoids matching `name=` inside
/// `filename=`.
fn disposition_value(line: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=");
    for segment in line.split(';') {
        let segment = segment.trim();
        if segment.to_ascii_lowercase().starts_with(&needle) {
            return Some(unquote(&segment[needle.len()..]).to_string());
        }
    }
    None
}

/// Strip one pair of surrounding double quotes, if present.
fn unquote(s: &str) -> &str {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

fn find_all(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    if needle.is_empty() || needle.len() > haystack.len() {
        return out;
    }
    let mut i = 0;
    while i + needle.len() <= haystack.len() {
        if &haystack[i..i + needle.len()] == needle {
            out.push(i);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Misc
// ---------------------------------------------------------------------------

/// Lock a mutex, recovering from poisoning so one panic cannot wedge the server.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn parse_args(args: &[String]) -> Result<Config, String> {
    let mut port = None;
    let mut library = None;
    let mut seeds = Vec::new();
    let mut wasm = PathBuf::from(DEFAULT_WASM);

    let mut index = 0;
    while index < args.len() {
        let arg = args[index].clone();
        let (key, inline) = match arg.split_once('=') {
            Some((key, value)) => (key.to_string(), Some(value.to_string())),
            None => (arg.clone(), None),
        };
        match key.as_str() {
            "--port" => {
                let value = next_value(args, &inline, &mut index, "--port")?;
                port = Some(
                    value
                        .parse::<u16>()
                        .map_err(|_| "invalid --port".to_string())?,
                );
            }
            "--library" => {
                let value = next_value(args, &inline, &mut index, "--library")?;
                library = Some(PathBuf::from(value));
            }
            "--seed" => {
                let value = next_value(args, &inline, &mut index, "--seed")?;
                seeds.push(PathBuf::from(value));
            }
            "--wasm" => {
                let value = next_value(args, &inline, &mut index, "--wasm")?;
                wasm = PathBuf::from(value);
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
        index += 1;
    }

    Ok(Config {
        port: port.ok_or("missing --port")?,
        library: library.ok_or("missing --library")?,
        seeds,
        wasm,
    })
}

fn next_value(
    args: &[String],
    inline: &Option<String>,
    index: &mut usize,
    key: &str,
) -> Result<String, String> {
    if let Some(value) = inline {
        return Ok(value.clone());
    }
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{key} requires a value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_must_be_lowercase_hex64() {
        assert!(valid_id(&"a".repeat(64)));
        assert!(valid_id(&"0123456789abcdef".repeat(4)));
        assert!(!valid_id(&"A".repeat(64)));
        assert!(!valid_id(&"a".repeat(63)));
        assert!(!valid_id(&"g".repeat(64)));
        assert!(!valid_id(""));
    }

    #[test]
    fn mapper_names_are_known_case_insensitively() {
        assert!(is_mapper_name("MBC5"));
        assert!(is_mapper_name("mbc5"));
        assert!(is_mapper_name("ROM"));
        assert!(is_mapper_name("unknown"));
        assert!(!is_mapper_name("NOPE"));
    }

    #[test]
    fn multipart_roundtrip() {
        let boundary = "XyZ123";
        let mut body = Vec::new();
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"rom\"; \
                 filename=\"game.gb\"\r\nContent-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(b"ROMBYTES");
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

        let parts = parse_multipart(&body, boundary);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].name, "rom");
        assert_eq!(parts[0].filename.as_deref(), Some("game.gb"));
        assert_eq!(parts[0].data, b"ROMBYTES");
    }

    #[test]
    fn multipart_without_rom_part() {
        let boundary = "abc";
        let body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"other\"\r\n\r\nx\r\n--{boundary}--\r\n"
        );
        let parts = parse_multipart(body.as_bytes(), boundary);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].name, "other");
    }

    #[test]
    fn boundary_is_extracted() {
        assert_eq!(
            multipart_boundary("multipart/form-data; boundary=----WebKitFormBoundaryABC"),
            Some("----WebKitFormBoundaryABC".to_string())
        );
        assert_eq!(
            multipart_boundary("multipart/form-data; boundary=\"quoted\""),
            Some("quoted".to_string())
        );
        assert_eq!(multipart_boundary("multipart/form-data"), None);
    }

    #[test]
    fn args_parse() {
        let args: Vec<String> = [
            "--port",
            "8080",
            "--library",
            "/tmp/lib",
            "--seed",
            "a",
            "--seed=b",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let config = parse_args(&args).expect("valid args");
        assert_eq!(config.port, 8080);
        assert_eq!(config.library, PathBuf::from("/tmp/lib"));
        assert_eq!(config.seeds, vec![PathBuf::from("a"), PathBuf::from("b")]);
        assert!(parse_args(&["--port".to_string(), "1".to_string()]).is_err());
        assert!(parse_args(&["--bogus".to_string()]).is_err());
    }

    #[test]
    fn attachment_names_are_safe() {
        assert_eq!(attachment_name("game.gb"), "game.gb");
        assert_eq!(attachment_name("a\"b\\c"), "a_b_c");
        assert_eq!(attachment_name(""), "rom.gb");
    }

    #[test]
    fn unquote_strips_pair() {
        assert_eq!(unquote("\"x\""), "x");
        assert_eq!(unquote("x"), "x");
        assert_eq!(unquote("\""), "\"");
    }
}
