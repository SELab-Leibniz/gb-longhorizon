//! `gb-server` — a tiny HTTP debugger around one [`Emulator`] instance.
//!
//! ```text
//! gb-server --port P [--doctor]
//! ```
//!
//! Speaks HTTP/1.1 on `127.0.0.1:P` (`Connection: close`), with the JSON
//! endpoints described in GEP-0001 Appendix B. Zero dependencies: the HTTP
//! framing, the JSON codec (`gb_tools::json`) and the disassembler
//! (`gb_tools::disasm`) are all hand-rolled. The server must survive any
//! malformed request without exiting, so request parsing and dispatch are
//! wrapped in `catch_unwind`.

// Some functions in this crate are stubs (`todo!()`, see their doc comments); the
// helpers they used are still here, so they show up as unused until the stubs are
// implemented again. Remove this allow when they are.
#![allow(dead_code, unused_imports)]

use std::collections::{BTreeSet, HashMap};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

use gb_core::cpu::Registers;
use gb_core::util::{fnv1a64, rgb555_bytes};
use gb_core::{Buttons, DataAccess, Emulator, Model, StepResult, CYCLES_PER_FRAME};
use gb_tools::json::Json;
use gb_tools::{disasm, parse_model, parse_number};

const USAGE: &str = "usage: gb-server --port P [--doctor]";

/// Largest request header we will buffer before rejecting the request.
const MAX_HEADER: usize = 1 << 20;
/// Largest request body we will buffer.
const MAX_BODY: usize = 16 << 20;
/// Upper bound on instructions that produce no trace line before `/step`
/// gives up, so a HALTed ROM cannot spin forever.
const MAX_SPIN: u64 = 50_000_000;

// ---------------------------------------------------------------------------
// JSON helpers
// ---------------------------------------------------------------------------

/// Build a JSON object from its fields (order preserved).
fn obj(fields: Vec<(&str, Json)>) -> Json {
    Json::Object(
        fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

/// A convenient `Int` for the many 8/16-bit quantities in the API.
fn int(v: i64) -> Json {
    Json::Int(v)
}

/// A convenient `Str`.
fn jstr(v: &str) -> Json {
    Json::Str(v.to_string())
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

/// An HTTP response: a status code, a JSON body and whether it asked for the
/// process to shut down.
struct Response {
    status: u16,
    body: String,
    shutdown: bool,
}

impl Response {
    fn json(value: Json) -> Self {
        Self {
            status: 200,
            body: value.to_string(),
            shutdown: false,
        }
    }

    fn ok() -> Self {
        Self::json(obj(vec![("ok", Json::Bool(true))]))
    }

    fn error(status: u16, message: &str) -> Self {
        Self {
            status,
            body: obj(vec![("error", jstr(message))]).to_string(),
            shutdown: false,
        }
    }

    fn shutdown() -> Self {
        Self {
            status: 200,
            body: obj(vec![("ok", Json::Bool(true))]).to_string(),
            shutdown: true,
        }
    }
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        409 => "Conflict",
        500 => "Internal Server Error",
        _ => "Error",
    }
}

/// A parsed request line plus body.
struct Request {
    method: String,
    target: String,
    body: Vec<u8>,
}

/// Find `needle` inside `haystack`.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Read one request from `stream`. Never panics: malformed input becomes an
/// error that the caller turns into a `400`.
fn read_request(stream: &mut TcpStream) -> Result<Request, String> {
    let mut buf: Vec<u8> = Vec::new();
    let header_end = loop {
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
        if buf.len() > MAX_HEADER {
            return Err("request header too large".to_string());
        }
        let mut chunk = [0u8; 4096];
        let n = stream
            .read(&mut chunk)
            .map_err(|e| format!("read error: {e}"))?;
        if n == 0 {
            return Err("malformed request: no header terminator".to_string());
        }
        buf.extend_from_slice(&chunk[..n]);
    };

    let header = std::str::from_utf8(&buf[..header_end])
        .map_err(|_| "malformed request: header is not UTF-8".to_string())?;
    let mut lines = header.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "malformed request: empty".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "malformed request line".to_string())?
        .to_ascii_uppercase();
    let target = parts
        .next()
        .ok_or_else(|| "malformed request line".to_string())?
        .to_string();

    let mut content_length = 0usize;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "malformed Content-Length".to_string())?;
            }
        }
    }
    if content_length > MAX_BODY {
        return Err("request body too large".to_string());
    }

    let mut body = buf[header_end..].to_vec();
    while body.len() < content_length {
        let mut chunk = [0u8; 4096];
        let n = stream
            .read(&mut chunk)
            .map_err(|e| format!("read error: {e}"))?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(content_length);

    Ok(Request {
        method,
        target,
        body,
    })
}

fn write_response(stream: &mut TcpStream, response: &Response) -> std::io::Result<()> {
    let body = response.body.as_bytes();
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        status_text(response.status),
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

// ---------------------------------------------------------------------------
// Query strings
// ---------------------------------------------------------------------------

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => (percent_decode(key), percent_decode(value)),
            None => (percent_decode(pair), String::new()),
        })
        .collect()
}

/// Split a request target into its path and decoded query parameters.
fn split_target(target: &str) -> (String, Vec<(String, String)>) {
    match target.split_once('?') {
        Some((path, query)) => (percent_decode(path), parse_query(query)),
        None => (percent_decode(target), Vec::new()),
    }
}

fn qparam<'a>(query: &'a [(String, String)], key: &str) -> Option<&'a str> {
    query
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

fn q_u16(query: &[(String, String)], key: &str) -> Result<Option<u16>, String> {
    match qparam(query, key) {
        None => Ok(None),
        Some(value) => match parse_number(value) {
            Some(n) if (0..=65535).contains(&n) => Ok(Some(n as u16)),
            _ => Err(format!("bad `{key}`")),
        },
    }
}

fn q_len(query: &[(String, String)], key: &str) -> Result<Option<u64>, String> {
    match qparam(query, key) {
        None => Ok(None),
        Some(value) => match parse_number(value) {
            Some(n) if n >= 0 => Ok(Some(n as u64)),
            _ => Err(format!("bad `{key}`")),
        },
    }
}

fn q_i64(query: &[(String, String)], key: &str) -> Result<Option<i64>, String> {
    match qparam(query, key) {
        None => Ok(None),
        Some(value) => parse_number(value)
            .map(Some)
            .ok_or_else(|| format!("bad `{key}`")),
    }
}

// ---------------------------------------------------------------------------
// Hex
// ---------------------------------------------------------------------------

fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn hex_decode(text: &str) -> Result<Vec<u8>, String> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) {
        return Err("hex string must have an even length".to_string());
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(text.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_val(bytes[i]).ok_or_else(|| "invalid hex digit".to_string())?;
        let lo = hex_val(bytes[i + 1]).ok_or_else(|| "invalid hex digit".to_string())?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Body / typed fields
// ---------------------------------------------------------------------------

/// Parse the request body as JSON; an empty body is an empty object.
fn body_json(request: &Request) -> Result<Json, String> {
    if request.body.is_empty() {
        return Ok(Json::Object(Vec::new()));
    }
    gb_tools::json::parse(&request.body)
}

fn int_field(body: &Json, key: &str, lo: i64, hi: i64) -> Result<i64, String> {
    let n = body
        .get(key)
        .and_then(Json::as_int)
        .ok_or_else(|| format!("missing `{key}`"))?;
    if !(lo..=hi).contains(&n) {
        return Err(format!("`{key}` out of range"));
    }
    Ok(n)
}

fn byte_field(body: &Json, key: &str) -> Result<Option<u8>, String> {
    match body.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(value) => match value.as_int() {
            Some(n) if (0..=255).contains(&n) => Ok(Some(n as u8)),
            _ => Err(format!("`{key}` must be a byte (0..=255)")),
        },
    }
}

fn word_field(body: &Json, key: &str) -> Result<Option<u16>, String> {
    match body.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(value) => match value.as_int() {
            Some(n) if (0..=65535).contains(&n) => Ok(Some(n as u16)),
            _ => Err(format!("`{key}` must be a word (0..=65535)")),
        },
    }
}

fn bool_field(body: &Json, key: &str) -> Result<Option<bool>, String> {
    match body.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(value) => match value.as_bool() {
            Some(b) => Ok(Some(b)),
            None => Err(format!("`{key}` must be a boolean")),
        },
    }
}

// ---------------------------------------------------------------------------
// ROM / model / buttons
// ---------------------------------------------------------------------------

/// The header title, matching `Cartridge::parse`: stop at the first NUL and
/// replace non-printable bytes with `?`.
fn rom_title(rom: &[u8]) -> String {
    rom.get(0x134..0x144)
        .unwrap_or(&[])
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                b as char
            } else {
                '?'
            }
        })
        .collect()
}

fn model_str(model: Model) -> &'static str {
    match model {
        Model::Dmg => "dmg",
        Model::Cgb => "cgb",
    }
}

/// Build the joypad state from a JSON array of button names (case-insensitive).
fn buttons_from_json(value: &Json) -> Result<Buttons, String> {
    let items = value
        .as_array()
        .ok_or_else(|| "`buttons` must be an array".to_string())?;
    let mut names = Vec::with_capacity(items.len());
    for item in items {
        let name = item
            .as_str()
            .ok_or_else(|| "button names must be strings".to_string())?;
        names.push(name.to_string());
    }
    Buttons::parse_list(&names.join(","))
}

// ---------------------------------------------------------------------------
// Watchpoints
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum WatchKind {
    Read,
    Write,
}

fn kind_str(kind: WatchKind) -> &'static str {
    match kind {
        WatchKind::Read => "read",
        WatchKind::Write => "write",
    }
}

fn watch_kind_from(name: &str) -> Option<WatchKind> {
    match name.to_ascii_lowercase().as_str() {
        "read" => Some(WatchKind::Read),
        "write" => Some(WatchKind::Write),
        _ => None,
    }
}

fn watchpoint_from(body: &Json) -> Result<(u16, WatchKind), String> {
    let addr = int_field(body, "addr", 0, 65535)? as u16;
    let name = body
        .get("kind")
        .and_then(Json::as_str)
        .ok_or_else(|| "missing `kind`".to_string())?;
    let kind = watch_kind_from(name).ok_or_else(|| format!("unknown watch kind {name:?}"))?;
    Ok((addr, kind))
}

// ---------------------------------------------------------------------------
// Instruction counting
// ---------------------------------------------------------------------------

/// Emit a trace line for the instruction about to run? Not while HALTed and
/// not for an interrupt dispatch that would be serviced instead of it (this
/// mirrors `gb-trace`).
fn line_pending(emu: &Emulator) -> bool {
    if emu.is_halted() {
        return false;
    }
    let pending = emu.ime() && (emu.peek(0xFF0F) & emu.peek(0xFFFF) & 0x1F) != 0;
    !pending
}

/// What one `core_step` observed.
struct CoreStep {
    /// PC of the instruction that was about to run.
    pc: u16,
    /// Whether the instruction produced a trace line.
    visible: bool,
    /// Data-bus accesses of that instruction.
    accesses: Vec<DataAccess>,
}

/// Set IME through the save-state blob (the only public door into `Cpu::ime`).
fn apply_ime(emu: &mut Emulator, on: bool) {
    let mut state = emu.save_state();
    // cpu::save_state layout: 8 regs, sp(2), pc(2), then ime at offset 12.
    if state.len() > 12 {
        state[12] = on as u8;
        let _ = emu.load_state(&state);
    }
}

// ---------------------------------------------------------------------------
// The server
// ---------------------------------------------------------------------------

struct Server {
    emu: Option<Emulator>,
    doctor: bool,
    breakpoints: BTreeSet<u16>,
    watchpoints: BTreeSet<(u16, WatchKind)>,
    /// Executed-instruction count per PC, since the last load/reset.
    profile: HashMap<u16, u64>,
    /// Total executed instructions that produced a trace line.
    visible_count: u64,
    /// Frames elapsed, tracked from instruction T-cycles since `step_frame`
    /// is not used by the debugger.
    frames: u64,
    /// T-cycles accumulated towards the next frame.
    accum: u32,
    /// Saved states by id, with the frame counter at save time.
    states: HashMap<String, (Vec<u8>, u64)>,
    /// A breakpoint we stopped on and have not executed yet: `/run` skips it
    /// once so resuming executes that instruction first.
    pending_skip: Option<u16>,
}

impl Server {
    fn new(doctor: bool) -> Self {
        Self {
            emu: None,
            doctor,
            breakpoints: BTreeSet::new(),
            watchpoints: BTreeSet::new(),
            profile: HashMap::new(),
            visible_count: 0,
            frames: 0,
            accum: 0,
            states: HashMap::new(),
            pending_skip: None,
        }
    }

    /// Serve one connection. Returns `true` if the client asked to shut down.
    fn serve(&mut self, stream: &mut TcpStream) -> bool {
        let request = match read_request(stream) {
            Ok(request) => request,
            Err(message) => {
                let _ = write_response(stream, &Response::error(400, &message));
                return false;
            }
        };
        let response = match catch_unwind(AssertUnwindSafe(|| self.handle(&request))) {
            Ok(response) => response,
            Err(_) => Response::error(500, "internal error"),
        };
        let shutdown = response.shutdown;
        let _ = write_response(stream, &response);
        shutdown
    }

    fn current_pc(&self) -> u16 {
        self.emu.as_ref().map(|emu| emu.registers().pc).unwrap_or(0)
    }

    /// Execute one instruction, updating the frame/profile counters.
    fn core_step(&mut self) -> CoreStep {
        todo!("execute one instruction for /run and /step, recording breakpoint and watchpoint hits (Appendix B semantics)")
    }

    fn clear_counters(&mut self) {
        self.profile.clear();
        self.visible_count = 0;
        self.frames = 0;
        self.accum = 0;
    }

    /// Everything that is meaningless for a fresh ROM.
    fn clear_rom_session(&mut self) {
        self.breakpoints.clear();
        self.watchpoints.clear();
        self.states.clear();
        self.pending_skip = None;
        self.clear_counters();
    }

    fn registers_response(&self) -> Response {
        match self.emu.as_ref() {
            Some(emu) => registers_json(emu, self.frames),
            None => Response::error(409, "no ROM loaded"),
        }
    }

    fn breakpoints_response(&self) -> Response {
        let list = self.breakpoints.iter().map(|&pc| int(pc as i64)).collect();
        Response::json(obj(vec![("breakpoints", Json::Array(list))]))
    }

    fn watchpoints_response(&self) -> Response {
        let list = self
            .watchpoints
            .iter()
            .map(|(addr, kind)| {
                obj(vec![
                    ("addr", int(*addr as i64)),
                    ("kind", jstr(kind_str(*kind))),
                ])
            })
            .collect();
        Response::json(obj(vec![("watchpoints", Json::Array(list))]))
    }

    fn run_response(
        &self,
        stopped: &str,
        pc: u16,
        watch: Option<(u16, WatchKind, u8)>,
    ) -> Response {
        let watch = match watch {
            None => Json::Null,
            Some((addr, kind, value)) => obj(vec![
                ("addr", int(addr as i64)),
                ("kind", jstr(kind_str(kind))),
                ("value", int(value as i64)),
                ("pc", int(pc as i64)),
            ]),
        };
        Response::json(obj(vec![
            ("stopped", jstr(stopped)),
            ("pc", int(pc as i64)),
            ("frames", int(self.frames as i64)),
            ("watch", watch),
        ]))
    }

    /// The first data access that hits a watchpoint, if any.
    fn match_watch(&self, accesses: &[DataAccess]) -> Option<(u16, WatchKind, u8)> {
        for access in accesses {
            let kind = if access.write {
                WatchKind::Write
            } else {
                WatchKind::Read
            };
            if self.watchpoints.contains(&(access.addr, kind)) {
                return Some((access.addr, kind, access.value));
            }
        }
        None
    }

    // -- dispatch -----------------------------------------------------------

    fn handle(&mut self, request: &Request) -> Response {
        let (path, query) = split_target(&request.target);
        let method = request.method.as_str();

        // Always available, even before a ROM is loaded.
        if method == "GET" && path == "/health" {
            return Response::ok();
        }
        if method == "POST" && path == "/load" {
            return self.handle_load(request);
        }
        if method == "POST" && path == "/shutdown" {
            return Response::shutdown();
        }

        if self.emu.is_none() {
            if is_protected(&path) {
                return Response::error(409, "no ROM loaded");
            }
            return Response::error(404, "unknown endpoint");
        }

        match (method, path.as_str()) {
            ("POST", "/reset") => self.handle_reset(),
            ("GET", "/registers") => self.registers_response(),
            ("POST", "/registers") => self.handle_set_registers(request),
            ("POST", "/step") => self.handle_step(request),
            ("POST", "/run") => self.handle_run(request),
            ("GET", "/breakpoints") => self.breakpoints_response(),
            ("POST", "/breakpoints") => self.handle_add_breakpoint(request),
            ("DELETE", p) if p.starts_with("/breakpoints/") => self.handle_del_breakpoint(p),
            ("GET", "/watchpoints") => self.watchpoints_response(),
            ("POST", "/watchpoints") => self.handle_add_watchpoint(request),
            ("DELETE", "/watchpoints") => self.handle_del_watchpoint(request),
            ("GET", "/memory") => self.handle_memory_get(&query),
            ("POST", "/memory") => self.handle_memory_post(request),
            ("GET", "/disassemble") => self.handle_disassemble(&query),
            ("GET", "/screenshot") => self.handle_screenshot(),
            ("POST", "/input") => self.handle_input(request),
            ("POST", "/state/save") => self.handle_state_save(),
            ("POST", "/state/load") => self.handle_state_load(request),
            ("GET", "/profile") => self.handle_profile(&query),
            _ => Response::error(404, "unknown endpoint"),
        }
    }

    // -- endpoints ----------------------------------------------------------

    fn handle_load(&mut self, request: &Request) -> Response {
        let body = match body_json(request) {
            Ok(body) => body,
            Err(message) => return Response::error(400, &message),
        };
        let path = match body.get("path").and_then(Json::as_str) {
            Some(path) => path.to_string(),
            None => return Response::error(400, "missing `path`"),
        };
        let rom = match std::fs::read(&path) {
            Ok(rom) => rom,
            Err(error) => return Response::error(400, &format!("cannot read {path}: {error}")),
        };
        let model = match body.get("model") {
            None | Some(Json::Null) => Model::Dmg,
            Some(value) => {
                let name = match value.as_str() {
                    Some(name) => name,
                    None => return Response::error(400, "`model` must be a string"),
                };
                let lower = name.to_ascii_lowercase();
                match lower.as_str() {
                    "auto" => Model::for_rom(&rom),
                    other => match parse_model(other) {
                        Some(model) => model,
                        None => return Response::error(400, &format!("unknown model {other:?}")),
                    },
                }
            }
        };
        let mut emu = match Emulator::load_with_model(&rom, model) {
            Ok(emu) => emu,
            Err(error) => return Response::error(400, &format!("cannot load {path}: {error}")),
        };
        if self.doctor {
            emu.set_doctor(true);
        }
        let title = rom_title(&rom);

        self.clear_rom_session();
        self.emu = Some(emu);
        Response::json(obj(vec![
            ("ok", Json::Bool(true)),
            ("title", jstr(&title)),
            ("model", jstr(model_str(model))),
        ]))
    }

    fn handle_reset(&mut self) -> Response {
        if let Some(emu) = self.emu.as_mut() {
            emu.reset();
            if self.doctor {
                emu.set_doctor(true);
            }
        }
        self.pending_skip = None;
        self.clear_counters();
        Response::ok()
    }

    fn handle_set_registers(&mut self, request: &Request) -> Response {
        macro_rules! field {
            ($body:expr, $function:ident, $key:literal) => {
                match $function(&$body, $key) {
                    Ok(value) => value,
                    Err(message) => return Response::error(400, &message),
                }
            };
        }
        let body = match body_json(request) {
            Ok(body) => body,
            Err(message) => return Response::error(400, &message),
        };
        let Some(emu) = self.emu.as_mut() else {
            return Response::error(409, "no ROM loaded");
        };
        let mut regs = emu.registers();
        if let Some(value) = field!(body, byte_field, "a") {
            regs.a = value;
        }
        if let Some(value) = field!(body, byte_field, "f") {
            regs.f = value;
        }
        if let Some(value) = field!(body, byte_field, "b") {
            regs.b = value;
        }
        if let Some(value) = field!(body, byte_field, "c") {
            regs.c = value;
        }
        if let Some(value) = field!(body, byte_field, "d") {
            regs.d = value;
        }
        if let Some(value) = field!(body, byte_field, "e") {
            regs.e = value;
        }
        if let Some(value) = field!(body, byte_field, "h") {
            regs.h = value;
        }
        if let Some(value) = field!(body, byte_field, "l") {
            regs.l = value;
        }
        if let Some(value) = field!(body, word_field, "sp") {
            regs.sp = value;
        }
        if let Some(value) = field!(body, word_field, "pc") {
            regs.pc = value;
        }
        regs.f &= 0xF0;
        emu.set_registers(regs);
        if let Some(value) = field!(body, bool_field, "ime") {
            apply_ime(emu, value);
        }
        self.registers_response()
    }

    fn handle_step(&mut self, request: &Request) -> Response {
        let body = match body_json(request) {
            Ok(body) => body,
            Err(message) => return Response::error(400, &message),
        };
        let count = match body.get("instructions") {
            None | Some(Json::Null) => 1u64,
            Some(value) => match value.as_int() {
                Some(n) if n >= 0 => n as u64,
                _ => return Response::error(400, "`instructions` must be a non-negative integer"),
            },
        };
        self.pending_skip = None;

        let mut done = 0u64;
        let mut spin = 0u64;
        while done < count {
            let step = self.core_step();
            if step.visible {
                done += 1;
                spin = 0;
            } else {
                spin += 1;
                if spin >= MAX_SPIN {
                    break;
                }
            }
        }
        self.registers_response()
    }

    #[allow(unused_variables)]
    fn handle_run(&mut self, request: &Request) -> Response {
        todo!("POST /run: run up to n frames, stopping at breakpoints (before) and watchpoints (after) — Appendix B")
    }

    fn handle_add_breakpoint(&mut self, request: &Request) -> Response {
        let body = match body_json(request) {
            Ok(body) => body,
            Err(message) => return Response::error(400, &message),
        };
        let pc = match int_field(&body, "pc", 0, 65535) {
            Ok(value) => value as u16,
            Err(message) => return Response::error(400, &message),
        };
        self.breakpoints.insert(pc);
        self.breakpoints_response()
    }

    fn handle_del_breakpoint(&mut self, segment: &str) -> Response {
        let text = segment.strip_prefix("/breakpoints/").unwrap_or(segment);
        let pc = match parse_number(text) {
            Some(n) if (0..=65535).contains(&n) => n as u16,
            _ => return Response::error(400, "bad breakpoint address"),
        };
        self.breakpoints.remove(&pc);
        self.breakpoints_response()
    }

    fn handle_add_watchpoint(&mut self, request: &Request) -> Response {
        let body = match body_json(request) {
            Ok(body) => body,
            Err(message) => return Response::error(400, &message),
        };
        match watchpoint_from(&body) {
            Ok(watchpoint) => {
                self.watchpoints.insert(watchpoint);
                self.watchpoints_response()
            }
            Err(message) => Response::error(400, &message),
        }
    }

    fn handle_del_watchpoint(&mut self, request: &Request) -> Response {
        let body = match body_json(request) {
            Ok(body) => body,
            Err(message) => return Response::error(400, &message),
        };
        match watchpoint_from(&body) {
            Ok(watchpoint) => {
                self.watchpoints.remove(&watchpoint);
                self.watchpoints_response()
            }
            Err(message) => Response::error(400, &message),
        }
    }

    fn handle_memory_get(&self, query: &[(String, String)]) -> Response {
        let addr = match q_u16(query, "addr") {
            Ok(Some(addr)) => addr,
            Ok(None) => return Response::error(400, "missing `addr`"),
            Err(message) => return Response::error(400, &message),
        };
        let len = match q_len(query, "len") {
            Ok(Some(len)) => len,
            Ok(None) => 0,
            Err(message) => return Response::error(400, &message),
        };
        if len > 65536 {
            return Response::error(400, "`len` too large (max 65536)");
        }
        let Some(emu) = self.emu.as_ref() else {
            return Response::error(409, "no ROM loaded");
        };
        let mut bytes = Vec::with_capacity(len as usize);
        for i in 0..len {
            bytes.push(emu.peek(addr.saturating_add(i as u16)));
        }
        Response::json(obj(vec![
            ("addr", int(addr as i64)),
            ("data", jstr(&hex_encode(&bytes))),
        ]))
    }

    fn handle_memory_post(&mut self, request: &Request) -> Response {
        let body = match body_json(request) {
            Ok(body) => body,
            Err(message) => return Response::error(400, &message),
        };
        let addr = match int_field(&body, "addr", 0, 65535) {
            Ok(addr) => addr as u16,
            Err(message) => return Response::error(400, &message),
        };
        let data = match body.get("data").and_then(Json::as_str) {
            Some(data) => data,
            None => return Response::error(400, "missing `data`"),
        };
        let bytes = match hex_decode(data) {
            Ok(bytes) => bytes,
            Err(message) => return Response::error(400, &message),
        };
        let Some(emu) = self.emu.as_mut() else {
            return Response::error(409, "no ROM loaded");
        };
        for (i, byte) in bytes.iter().enumerate() {
            emu.poke(addr.wrapping_add(i as u16), *byte);
        }
        Response::ok()
    }

    fn handle_disassemble(&self, query: &[(String, String)]) -> Response {
        let addr = match q_u16(query, "addr") {
            Ok(Some(addr)) => addr,
            Ok(None) => return Response::error(400, "missing `addr`"),
            Err(message) => return Response::error(400, &message),
        };
        let count = match q_len(query, "count") {
            Ok(Some(count)) => count,
            Ok(None) => 1,
            Err(message) => return Response::error(400, &message),
        };
        if count > 65536 {
            return Response::error(400, "`count` too large (max 65536)");
        }
        let Some(emu) = self.emu.as_ref() else {
            return Response::error(409, "no ROM loaded");
        };
        let mut pc = addr;
        let mut list = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let (len, mnemonic) = disasm::disassemble(|a| emu.peek(a), pc);
            let mut bytes = Vec::with_capacity(len as usize);
            for i in 0..len {
                bytes.push(emu.peek(pc.wrapping_add(i as u16)));
            }
            list.push(obj(vec![
                ("addr", int(pc as i64)),
                ("bytes", jstr(&hex_encode(&bytes))),
                ("text", jstr(&mnemonic)),
            ]));
            pc = pc.wrapping_add(len as u16);
        }
        Response::json(obj(vec![("instructions", Json::Array(list))]))
    }

    fn handle_screenshot(&self) -> Response {
        let Some(emu) = self.emu.as_ref() else {
            return Response::error(409, "no ROM loaded");
        };
        let (format, hash) = match emu.model() {
            Model::Dmg => ("dmg-shades", fnv1a64(emu.framebuffer())),
            Model::Cgb => (
                "cgb-rgb555",
                fnv1a64(&rgb555_bytes(emu.framebuffer_rgb555())),
            ),
        };
        Response::json(obj(vec![
            ("frames", int(self.frames as i64)),
            ("format", jstr(format)),
            ("hash", jstr(&format!("{hash:016x}"))),
        ]))
    }

    fn handle_input(&mut self, request: &Request) -> Response {
        let body = match body_json(request) {
            Ok(body) => body,
            Err(message) => return Response::error(400, &message),
        };
        let value = match body.get("buttons") {
            Some(value) => value,
            None => return Response::error(400, "missing `buttons`"),
        };
        let buttons = match buttons_from_json(value) {
            Ok(buttons) => buttons,
            Err(message) => return Response::error(400, &message),
        };
        let Some(emu) = self.emu.as_mut() else {
            return Response::error(409, "no ROM loaded");
        };
        emu.set_buttons(buttons);
        Response::ok()
    }

    fn handle_state_save(&mut self) -> Response {
        let Some(emu) = self.emu.as_ref() else {
            return Response::error(409, "no ROM loaded");
        };
        let blob = emu.save_state();
        let id = format!("{:016x}", fnv1a64(&blob));
        self.states.insert(id.clone(), (blob, self.frames));
        Response::json(obj(vec![("id", jstr(&id))]))
    }

    fn handle_state_load(&mut self, request: &Request) -> Response {
        let body = match body_json(request) {
            Ok(body) => body,
            Err(message) => return Response::error(400, &message),
        };
        let id = match body.get("id").and_then(Json::as_str) {
            Some(id) => id.to_string(),
            None => return Response::error(400, "missing `id`"),
        };
        let Some((blob, frames)) = self.states.get(&id).cloned() else {
            return Response::error(404, "unknown state id");
        };
        let Some(emu) = self.emu.as_mut() else {
            return Response::error(409, "no ROM loaded");
        };
        match emu.load_state(&blob) {
            Ok(()) => {
                self.frames = frames;
                self.accum = 0;
                self.pending_skip = None;
                Response::ok()
            }
            Err(error) => Response::error(400, &format!("bad state: {error}")),
        }
    }

    fn handle_profile(&self, query: &[(String, String)]) -> Response {
        let top = match q_i64(query, "top") {
            Ok(Some(n)) if n < 0 => return Response::error(400, "`top` must not be negative"),
            Ok(Some(n)) => n as usize,
            Ok(None) => 20,
            Err(message) => return Response::error(400, &message),
        };
        let mut entries: Vec<(u16, u64)> = self
            .profile
            .iter()
            .map(|(&pc, &count)| (pc, count))
            .collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let hot = entries
            .into_iter()
            .take(top)
            .map(|(pc, count)| obj(vec![("pc", int(pc as i64)), ("count", int(count as i64))]))
            .collect();
        Response::json(obj(vec![
            ("instructions", int(self.visible_count as i64)),
            ("hot", Json::Array(hot)),
        ]))
    }
}

/// Register file as the API's JSON object.
fn registers_json(emu: &Emulator, frames: u64) -> Response {
    let regs: Registers = emu.registers();
    Response::json(obj(vec![
        ("a", int(regs.a as i64)),
        ("f", int(regs.f as i64)),
        ("b", int(regs.b as i64)),
        ("c", int(regs.c as i64)),
        ("d", int(regs.d as i64)),
        ("e", int(regs.e as i64)),
        ("h", int(regs.h as i64)),
        ("l", int(regs.l as i64)),
        ("sp", int(regs.sp as i64)),
        ("pc", int(regs.pc as i64)),
        ("ime", Json::Bool(emu.ime())),
        ("halted", Json::Bool(emu.is_halted())),
        ("frames", int(frames as i64)),
    ]))
}

/// Endpoints that require a loaded ROM (everything except health/load/shutdown).
fn is_protected(path: &str) -> bool {
    matches!(
        path,
        "/reset"
            | "/registers"
            | "/step"
            | "/run"
            | "/breakpoints"
            | "/watchpoints"
            | "/memory"
            | "/disassemble"
            | "/screenshot"
            | "/input"
            | "/state/save"
            | "/state/load"
            | "/profile"
    ) || path.starts_with("/breakpoints/")
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn parse_args(argv: &[String]) -> Result<(u16, bool), String> {
    let mut port = None;
    let mut doctor = false;
    let mut i = 0;
    while i < argv.len() {
        let raw = &argv[i];
        let (key, inline) = match raw.split_once('=') {
            Some((key, value)) => (key, Some(value)),
            None => (raw.as_str(), None),
        };
        match key {
            "--port" => {
                let value = match inline {
                    Some(value) => value.to_string(),
                    None => {
                        i += 1;
                        argv.get(i)
                            .cloned()
                            .ok_or_else(|| format!("--port needs a value\n{USAGE}"))?
                    }
                };
                port = Some(
                    value
                        .parse::<u16>()
                        .map_err(|_| format!("bad port {value:?}\n{USAGE}"))?,
                );
            }
            "--doctor" => doctor = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other:?}\n{USAGE}")),
        }
        i += 1;
    }
    let port = port.ok_or_else(|| format!("--port is required\n{USAGE}"))?;
    Ok((port, doctor))
}

fn run() -> Result<(), String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let (port, doctor) = parse_args(&argv)?;

    let listener = TcpListener::bind(("127.0.0.1", port))
        .map_err(|e| format!("cannot bind port {port}: {e}"))?;
    let mut server = Server::new(doctor);

    for incoming in listener.incoming() {
        let mut stream = match incoming {
            Ok(stream) => stream,
            Err(_) => continue,
        };
        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
        let shutdown =
            catch_unwind(AssertUnwindSafe(|| server.serve(&mut stream))).unwrap_or(false);
        if shutdown {
            break;
        }
    }
    Ok(())
}

fn main() {
    let code = match catch_unwind(run) {
        Ok(Ok(())) => 0,
        Ok(Err(message)) => {
            eprintln!("gb-server: {message}");
            1
        }
        Err(_) => {
            eprintln!("gb-server: internal error");
            2
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let bytes = [0x00u8, 0x1a, 0xff, 0x90];
        let encoded = hex_encode(&bytes);
        assert_eq!(encoded, "001aff90");
        assert_eq!(hex_decode(&encoded).unwrap(), bytes);
        assert!(hex_decode("abc").is_err());
        assert!(hex_decode("zz").is_err());
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("a%20b+c"), "a b c");
        assert_eq!(percent_decode("%C3%A9"), "é");
    }

    #[test]
    fn query_parsing() {
        let query = parse_query("addr=0xC000&len=16");
        assert_eq!(qparam(&query, "addr"), Some("0xC000"));
        assert_eq!(q_u16(&query, "addr").unwrap(), Some(0xC000));
        assert_eq!(q_len(&query, "len").unwrap(), Some(16));
        assert_eq!(qparam(&query, "missing"), None);
        let (path, params) = split_target("/memory?addr=1&len=2");
        assert_eq!(path, "/memory");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn protected_paths() {
        assert!(is_protected("/registers"));
        assert!(is_protected("/breakpoints/0x100"));
        assert!(!is_protected("/health"));
        assert!(!is_protected("/load"));
        assert!(!is_protected("/shutdown"));
    }

    #[test]
    fn buttons_and_watchpoints() {
        let buttons = Json::Array(vec![
            Json::Str("A".to_string()),
            Json::Str("start".to_string()),
        ]);
        let parsed = buttons_from_json(&buttons).unwrap();
        assert!(parsed.a && parsed.start && !parsed.b);
        assert!(buttons_from_json(&Json::Array(vec![Json::Str("nope".to_string())])).is_err());
        assert!(buttons_from_json(&Json::Int(3)).is_err());

        assert_eq!(watch_kind_from("READ"), Some(WatchKind::Read));
        assert_eq!(watch_kind_from("write"), Some(WatchKind::Write));
        assert_eq!(watch_kind_from("poke"), None);
    }

    #[test]
    fn titles() {
        let mut rom = vec![0u8; 0x150];
        rom[0x134..0x139].copy_from_slice(b"TITLE");
        assert_eq!(rom_title(&rom), "TITLE");
        rom[0x139] = 0x01;
        assert_eq!(rom_title(&rom), "TITLE?");
        let empty = vec![0u8; 0x150];
        assert_eq!(rom_title(&empty), "");
    }
}
