// SPDX-License-Identifier: MIT
//! The player's own local server: it serves one folder, a page's, on
//! loopback alone, and sends the two cross-origin headers on every answer,
//! so the page reads `crossOriginIsolated` true and its Worker may block on
//! shared memory. Without them the page says so in a sentence and stops.
//!
//! Std only, so it adds no dependency. One thread a connection, and each
//! answer closes its connection, so an idle browser socket holds no thread
//! for long. It binds `127.0.0.1` and no other address: no other machine
//! can reach it. It answers GET and HEAD of the folder's files and nothing
//! else; a path that leaves the folder, a folder's own path and a missing
//! file all answer 404, alike, so an answer tells nothing of what lies
//! outside. A path is taken as written, never percent-decoded: a page's own
//! file names hold no `%` and no space.
//!
//! Call: `serve <folder> [port]`, port 0 or none taking a free one. Stdout
//! gets one line, the address to open; the log's one INFO line,
//! `serve.listen`, names it too, and `RUST_LOG=debug` adds a `serve.answer`
//! line a request. Exits 2 on a wrong call, 1 when the folder or the port
//! cannot be had.
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use log::LevelFilter;

/// The two headers that make a page cross-origin isolated, with their values.
const ISOLATION: [(&str, &str); 2] = [
    ("Cross-Origin-Opener-Policy", "same-origin"),
    ("Cross-Origin-Embedder-Policy", "require-corp"),
];

/// How long a connection may take to send its request before it is dropped.
const READ_LIMIT: Duration = Duration::from_secs(5);

/// The most header lines read from one request; the rest go unread.
const MOST_LINES: usize = 100;

/// One answer: its status, its body's type, and its body. A HEAD sends the
/// body's length in its header and no body.
#[derive(Debug, PartialEq, Eq)]
struct Answer {
    status: u16,
    kind: &'static str,
    body: Vec<u8>,
}

impl Answer {
    /// An answer with no body, for a refusal.
    fn bare(status: u16) -> Self {
        Answer {
            status,
            kind: "text/plain; charset=utf-8",
            body: Vec::new(),
        }
    }
}

/// A request's method and path from its first line, the query and fragment
/// dropped; `None` when the line is not `METHOD TARGET HTTP/version`.
fn parse(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.split_ascii_whitespace();
    let (method, target, version) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || !version.starts_with("HTTP/") {
        return None;
    }
    let path = target.split(['?', '#']).next().unwrap_or_default();
    Some((method, path))
}

/// The folder's file a path names, `/` naming `index.html`; `None` when the
/// path does not start with `/`, a part of it is empty, `.` or `..` or holds
/// a `\`, it names no file, or a link takes it out of the folder. `root` is
/// already canonical.
fn resolve(root: &Path, path: &str) -> Option<PathBuf> {
    let rest = path.strip_prefix('/')?;
    let rest = if rest.is_empty() { "index.html" } else { rest };
    let mut file = root.to_path_buf();
    for part in rest.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains('\\') {
            return None;
        }
        file.push(part);
    }
    let file = file.canonicalize().ok()?;
    (file.starts_with(root) && file.is_file()).then_some(file)
}

/// The body's type by the file's extension, for the kinds a page serves.
fn kind(file: &Path) -> &'static str {
    match file.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        Some("txt") => "text/plain; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    }
}

/// The answer to a request for `path` by `method`: the file whole, or a
/// refusal with no body.
fn answer(root: &Path, method: &str, path: &str) -> Answer {
    if method != "GET" && method != "HEAD" {
        return Answer::bare(405);
    }
    match resolve(root, path).and_then(|file| Some((kind(&file), fs::read(&file).ok()?))) {
        Some((kind, body)) => Answer {
            status: 200,
            kind,
            body,
        },
        None => Answer::bare(404),
    }
}

/// The answer's status line and headers, the two isolation headers among
/// them whatever the status, ended by the blank line.
fn head(answer: &Answer) -> String {
    let reason = match answer.status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "",
    };
    let allow = if answer.status == 405 {
        "Allow: GET, HEAD\r\n"
    } else {
        ""
    };
    let [(opener, opener_value), (embedder, embedder_value)] = ISOLATION;
    format!(
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
         {opener}: {opener_value}\r\n{embedder}: {embedder_value}\r\n\
         Cross-Origin-Resource-Policy: same-origin\r\nX-Content-Type-Options: nosniff\r\n\
         Cache-Control: no-store\r\n{allow}Connection: close\r\n\r\n",
        answer.status,
        answer.kind,
        answer.body.len(),
    )
}

/// Reads one request from the connection and writes its answer, then lets
/// the connection close. Writes one `serve.answer` DEBUG line; a connection
/// that sends no request line in time is dropped with no line.
fn handle(root: &Path, stream: &TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(READ_LIMIT))?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut header = String::new();
    for _ in 0..MOST_LINES {
        header.clear();
        if reader.read_line(&mut header)? == 0 || header.trim_end().is_empty() {
            break;
        }
    }
    let (method, path, reply) = match parse(&line) {
        Some((method, path)) => (method, path, answer(root, method, path)),
        None => ("?", "?", Answer::bare(400)),
    };
    log::debug!(m = method, p = path, s = reply.status, n = reply.body.len(); "serve.answer");
    let mut out = stream;
    out.write_all(head(&reply).as_bytes())?;
    if method != "HEAD" {
        out.write_all(&reply.body)?;
    }
    out.flush()
}

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let folder = args.next();
    let port = args.next().map_or(Some(0), |port| {
        port.to_str().and_then(|port| port.parse::<u16>().ok())
    });
    let (Some(folder), Some(port), None) = (folder, port, args.next()) else {
        eprintln!("call: serve <folder> [port]");
        return ExitCode::from(2);
    };
    let folder = PathBuf::from(folder);
    let cwd = std::env::current_dir().unwrap_or_default();
    let roots = oxide_logline::Roots {
        job: &folder,
        base: &folder,
        cwd: &cwd,
    };
    if oxide_logline::install("serve", &roots, std::io::stderr(), LevelFilter::Info).is_err() {
        eprintln!("serve: a logger was already installed");
        return ExitCode::FAILURE;
    }
    let root = match folder.canonicalize() {
        Ok(root) if root.is_dir() => root,
        Ok(_) => {
            log::error!(w = "folder", e = "NotADirectory"; "serve.failed");
            return ExitCode::FAILURE;
        }
        Err(error) => {
            log::error!(w = "folder", e = format!("{:?}", error.kind()).as_str(); "serve.failed");
            return ExitCode::FAILURE;
        }
    };
    let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
        Ok(listener) => listener,
        Err(error) => {
            log::error!(w = "port", e = format!("{:?}", error.kind()).as_str(); "serve.failed");
            return ExitCode::FAILURE;
        }
    };
    let address = match listener.local_addr() {
        Ok(local) => format!("http://{local}/"),
        Err(error) => {
            log::error!(w = "port", e = format!("{:?}", error.kind()).as_str(); "serve.failed");
            return ExitCode::FAILURE;
        }
    };
    log::info!(a = address.as_str(), r = root.display().to_string().as_str(); "serve.listen");
    println!("{address}");
    let root = Arc::new(root);
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let root = Arc::clone(&root);
        std::thread::spawn(move || {
            // A connection that fails mid-answer is the browser's to retry.
            let _ = handle(&root, &stream);
        });
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// A fresh page folder of two files under the system's temporary folder,
    /// with a file beside it, outside, that no path may reach.
    fn folder(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("serve-test-{}-{name}", std::process::id()));
        let page = base.join("page");
        fs::create_dir_all(page.join("sub")).expect("make the page folder");
        fs::write(page.join("index.html"), "<p>page</p>").expect("write the page");
        fs::write(page.join("sub/run.wasm"), b"\0asm").expect("write the module");
        fs::write(base.join("outside.txt"), "outside").expect("write the outside file");
        page.canonicalize().expect("the page folder is canonical")
    }

    #[test]
    fn serves_the_folder_files_by_kind() {
        let root = folder("kind");
        let page = answer(&root, "GET", "/");
        assert_eq!(
            (page.status, page.kind, page.body.as_slice()),
            (200, "text/html; charset=utf-8", b"<p>page</p>".as_slice())
        );
        let module = answer(&root, "HEAD", "/sub/run.wasm");
        assert_eq!((module.status, module.kind), (200, "application/wasm"));
        assert_eq!(
            parse("GET /sub/run.wasm?v=1#top HTTP/1.1"),
            Some(("GET", "/sub/run.wasm"))
        );
    }

    #[test]
    fn refuses_every_path_out_of_the_folder_alike() {
        let root = folder("refuse");
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("../outside.txt"), root.join("link.txt"))
            .expect("make the link");
        for path in [
            "/../outside.txt",
            "/sub/../../outside.txt",
            "/sub",
            "/missing.html",
            "//index.html",
            "/./index.html",
            "/sub\\run.wasm",
            "index.html",
            "/link.txt",
        ] {
            assert_eq!(answer(&root, "GET", path), Answer::bare(404), "{path}");
        }
    }

    #[test]
    fn refuses_other_methods_and_broken_lines() {
        let root = folder("method");
        let refused = answer(&root, "POST", "/");
        assert_eq!(refused, Answer::bare(405));
        assert!(head(&refused).contains("\r\nAllow: GET, HEAD\r\n"));
        for line in ["", "GET /", "GET / FTP/1.0", "GET / HTTP/1.1 more"] {
            assert_eq!(parse(line), None, "{line}");
        }
    }

    #[test]
    fn sends_both_isolation_headers_on_every_answer() {
        for status in [200, 400, 404, 405] {
            let text = head(&Answer::bare(status));
            for (name, value) in ISOLATION {
                assert!(
                    text.contains(&format!("\r\n{name}: {value}\r\n")),
                    "{status} {name}"
                );
            }
        }
    }

    #[test]
    fn answers_over_a_loopback_socket() {
        let root = folder("socket");
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind loopback");
        let address = listener.local_addr().expect("the bound address");
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept one connection");
            handle(&root, &stream).expect("answer it");
        });
        let mut client = TcpStream::connect(address).expect("connect");
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: page\r\n\r\n")
            .expect("send the request");
        let mut text = String::new();
        client
            .read_to_string(&mut text)
            .expect("read the answer to its close");
        server.join().expect("the server thread ends");
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"), "{text}");
        assert!(text.contains("\r\nCross-Origin-Opener-Policy: same-origin\r\n"));
        assert!(text.contains("\r\nCross-Origin-Embedder-Policy: require-corp\r\n"));
        assert!(text.ends_with("\r\n\r\n<p>page</p>"));
    }
}
