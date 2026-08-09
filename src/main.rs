use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn now_epoch() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

struct PadData {
    created: u64,
    body: String,
}

impl PadData {
    fn parse(raw: &str) -> Self {
        if let Some((headers, body)) = raw.split_once("\n---\n") {
            let created = headers.lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    if k.trim() == "created" { v.trim().parse().ok() } else { None }
                })
                .unwrap_or_else(now_epoch);
            PadData { created, body: body.to_string() }
        } else {
            PadData { created: now_epoch(), body: raw.to_string() }
        }
    }

    fn serialize(&self) -> String {
        format!("created: {}\n---\n{}", self.created, self.body)
    }
}

static INDEX_HTML: &str = include_str!("index.html");

struct AppState {
    data_dir: PathBuf,
}

impl AppState {
    fn new(data_dir: &str) -> Self {
        let path = PathBuf::from(data_dir);
        std::fs::create_dir_all(&path).expect("failed to create data directory");
        // migrate legacy extensionless pads to <id>.txt
        if let Ok(entries) = std::fs::read_dir(&path) {
            for e in entries.flatten() {
                let ft = match e.file_type() {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                if !ft.is_file() {
                    continue;
                }
                let name = e.file_name().to_string_lossy().to_string();
                if name.ends_with(".txt") || !is_valid_id(&name) {
                    continue;
                }
                let to = path.join(format!("{name}.txt"));
                let _ = std::fs::rename(e.path(), &to);
            }
        }
        AppState { data_dir: path }
    }

    fn pad_path(&self, id: &str) -> PathBuf {
        self.data_dir.join(format!("{id}.txt"))
    }

    fn get_pad(&self, id: &str) -> Option<String> {
        let raw = std::fs::read_to_string(self.pad_path(id)).ok()?;
        Some(PadData::parse(&raw).body)
    }

    fn write_pad(&self, id: &str, body: &str) {
        let created = std::fs::read_to_string(self.pad_path(id))
            .map(|raw| PadData::parse(&raw).created)
            .unwrap_or_else(|_| now_epoch());
        let data = PadData { created, body: body.to_string() };
        std::fs::write(self.pad_path(id), data.serialize()).expect("failed to write pad");
    }
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn parse_request(reader: &mut BufReader<TcpStream>) -> Option<Request> {
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    if line.is_empty() {
        return None;
    }

    let mut parts = line.trim_end_matches(|c| c == '\r' || c == '\n').splitn(3, ' ');
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();

    let mut content_length: usize = 0;
    loop {
        line.clear();
        reader.read_line(&mut line).ok()?;
        let trimmed = line.trim_end_matches(|c| c == '\r' || c == '\n');
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).ok()?;
    }

    Some(Request { method, path, body })
}

fn respond(stream: &mut TcpStream, status: u16, reason: &str, extra_headers: &str, body: &[u8]) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: keep-alive\r\n{extra_headers}\r\n",
        body.len()
    );
    let _ = stream.write_all(body);
}

fn handle(stream: TcpStream, state: Arc<AppState>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let mut reader = BufReader::new(stream);

    loop {
        let req = match parse_request(&mut reader) {
            Some(r) => r,
            None => break,
        };

        let path = req.path.split('?').next().unwrap_or(&req.path).to_string();
        let s = reader.get_mut();

        if req.method == "GET" && path == "/" {
            respond(s, 200, "OK", "Content-Type: text/html\r\n", INDEX_HTML.as_bytes());
        } else if req.method == "GET" && path == "/api/pads" {
            let mut ids: Vec<String> = std::fs::read_dir(&state.data_dir)
                .map(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .filter_map(|e| {
                            let name = e.file_name().to_string_lossy().to_string();
                            name.strip_suffix(".txt").map(|id| id.to_string())
                        })
                        .filter(|id| is_valid_id(id))
                        .collect()
                })
                .unwrap_or_default();
            ids.sort();
            let body = format!(
                "[{}]",
                ids.iter().map(|i| format!("\"{i}\"")).collect::<Vec<_>>().join(",")
            );
            respond(s, 200, "OK", "Content-Type: application/json\r\n", body.as_bytes());
        } else if let Some(rest) = path.strip_prefix("/api/pad/") {
            let (id, action) = match rest.rsplit_once('/') {
                Some((left, act)) => (left.to_string(), Some(act.to_string())),
                None => (rest.to_string(), None),
            };
            if !is_valid_id(&id) {
                respond(s, 404, "Not Found", "", b"");
            } else if action.as_deref() == Some("rename") {
                if req.method != "POST" {
                    respond(s, 405, "Method Not Allowed", "", b"");
                } else {
                    let new_id = std::str::from_utf8(&req.body).unwrap_or("").trim();
                    if !is_valid_id(new_id) {
                        respond(s, 400, "Bad Request", "", b"invalid name");
                    } else if new_id == id.as_str() {
                        respond(s, 200, "OK", "", b"");
                    } else if !state.pad_path(&id).exists() {
                        respond(s, 404, "Not Found", "", b"pad not found");
                    } else if state.pad_path(new_id).exists() {
                        respond(s, 409, "Conflict", "", b"name already exists");
                    } else {
                        match std::fs::rename(state.pad_path(&id), state.pad_path(new_id)) {
                            Ok(_) => respond(s, 200, "OK", "", b""),
                            Err(_) => respond(s, 500, "Internal Server Error", "", b"rename failed"),
                        }
                    }
                }
            } else if action.is_none() {
                if req.method == "GET" {
                    match state.get_pad(&id) {
                        Some(text) => respond(s, 200, "OK", "Content-Type: text/plain\r\n", text.as_bytes()),
                        None => respond(s, 404, "Not Found", "", b""),
                    }
                } else if req.method == "PUT" {
                    match std::str::from_utf8(&req.body) {
                        Ok(text) => {
                            state.write_pad(&id, text);
                            respond(s, 200, "OK", "", b"");
                        }
                        Err(_) => respond(s, 400, "Bad Request", "", b""),
                    }
                } else if req.method == "DELETE" {
                    match std::fs::remove_file(state.pad_path(&id)) {
                        Ok(_) => respond(s, 200, "OK", "", b""),
                        Err(_) => respond(s, 404, "Not Found", "", b""),
                    }
                } else {
                    respond(s, 405, "Method Not Allowed", "", b"");
                }
            } else {
                respond(s, 404, "Not Found", "", b"");
            }
        } else if req.method == "GET" {
            let id = path.trim_start_matches('/');
            if !is_valid_id(id) {
                respond(s, 404, "Not Found", "", b"");
            } else {
                respond(s, 200, "OK", "Content-Type: text/html\r\n", INDEX_HTML.as_bytes());
            }
        } else {
            respond(s, 404, "Not Found", "", b"");
        }
    }
}

fn main() {
    let data_dir = std::env::var("TPAD_DATA_DIR").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        format!("{home}/.local/share/tpad")
    });
    let state = Arc::new(AppState::new(&data_dir));
    let start_port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let listener = {
        let mut port = start_port;
        loop {
            match TcpListener::bind(format!("0.0.0.0:{port}")) {
                Ok(l) => { eprintln!("listening on http://0.0.0.0:{port}"); break l; }
                Err(_) if port < u16::MAX => port += 1,
                _ => panic!("no available port found"),
            }
        }
    };
    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            let state = Arc::clone(&state);
            std::thread::spawn(move || handle(stream, state));
        }
    }
}
