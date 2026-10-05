pub mod thread_pool;
use std::collections::HashMap;
use std::net::TcpStream;
use std::io::{Write, BufReader, BufRead, Read, ErrorKind};
use std::sync::Arc;
use std::path::{Path, PathBuf};
use flate2::{write::GzEncoder, Compression};
use std::fs::{self, OpenOptions};
use std::time::Duration;

const MAX_LINE: u64 = 8 * 1024;
const MAX_HEADERS: usize = 100;
const MAX_BODY: usize = 1024 * 1024;

#[derive(Eq, Hash, PartialEq, Copy, Clone)]
pub enum Method {
    GET,
    POST,
}

//Match method type string to enum
impl Method {
    fn parse(method: &str) -> Option<Method> {
        match method {
            "GET" => Some(Method::GET),
            "POST" => Some(Method::POST),
            _ => None,
        }
    }
}

pub enum ParseError { 
    Closed, 
    BadRequest, 
    MethodNotAllowed, 
    TooLarge 
}

pub enum Status { Ok,
    Created,
    BadRequest,
    NotFound,
    MethodNotAllowed,
    PayloadTooLarge,
    InternalServerError
}

impl Status {

    fn status_num(&self) -> u16 {
        match self {
            Status::Ok => 200,
            Status::Created => 201,
            Status::BadRequest => 400,
            Status::NotFound => 404,
            Status::MethodNotAllowed => 405,
            Status::PayloadTooLarge => 413,
            Status::InternalServerError => 500,
        }
    }

    fn reason(&self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::Created => "Created",
            Status::BadRequest => "Bad Request",
            Status::NotFound => "Not Found",
            Status::MethodNotAllowed => "Method Not Allowed",
            Status::PayloadTooLarge => "Payload Too Large",
            Status::InternalServerError => "Internal Server Error",
        }
    }
}

pub struct Request {
    pub method: Method,
    pub path: String,
    pub version: String,
    pub header: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub struct Response {
    pub status: Status,
    pub header: HashMap<&'static str, String>,
    pub body: Vec<u8>,
}

//Route table
pub fn route_table() -> Arc<HashMap<(Method, &'static str), fn(&Request) -> Response>> {

    let table: HashMap<(Method, &str), fn(&Request) -> Response> = [
        ((Method::GET, "/"), get_handle_root as fn(&Request) -> Response),
        ((Method::GET, "/about"), get_handle_root),
    ]
        .into_iter()
        .collect();

    Arc::new(table)
}

// Handle the request stream
// Goes to parse request to determine request type
// Passes to handle_request_dispatch which hands of to handler function that returns a Response
// instance
// Serialize response and write back to stream
pub fn handle_request(mut stream: TcpStream, table: Arc<HashMap<(Method, &'static str), fn(&Request) -> Response>>) -> Result<(), Box<dyn std::error::Error>> {

    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    
    let mut writer = stream.try_clone()?;
    //Create reader to read the buffer stream
    let mut reader = BufReader::new(&mut stream);
    loop {
        
        let request = match parse_request(&mut reader) {
            Ok(req) => req,
            Err(ParseError::Closed) => break,
            Err(e) => {
                let status = match e {
                    ParseError::MethodNotAllowed => Status::MethodNotAllowed,
                    ParseError::TooLarge => Status::PayloadTooLarge,
                    _ => Status::BadRequest,
                };
                let _ = writer.write_all(&error_bytes(status));
                break; // framing can't be trusted after a bad request
            }
        };

        let response = handle_request_dispatch(&request, &table);
        
        let start_line = format!("{} {} {}",request.version, response.status.status_num(), response.status.reason());

        let header = response.header
            .iter()
            .map(|(k,v)| format!("{k}: {v}"))
            .collect::<Vec<String>>()
            .join("\r\n");
        
        //Make message
        let mut message = format!("{start_line}\r\n{header}\r\n\r\n").into_bytes();
        message.extend_from_slice(&response.body);
        //Write back message
        writer.write_all(&message)?;

        //Check if keep connection open
        if is_close(&request) {
            break;
        }
    }

    Ok(())
}


fn read_line_limited<R: BufRead>(reader: &mut R) -> Result<String, ParseError> {
    let mut buf = Vec::new();
    let n = reader
        .by_ref()
        .take(MAX_LINE)
        .read_until(b'\n', &mut buf)
        .map_err(|_| ParseError::Closed)?;
    if n == 0 { return Err(ParseError::Closed); }
    if !buf.ends_with(b"\n") { return Err(ParseError::BadRequest); }
    let line = String::from_utf8(buf).map_err(|_| ParseError::BadRequest)?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

//Parse request into different sections
pub fn parse_request<R: BufRead>(reader: &mut R) -> Result<Request, ParseError> {

    let line = read_line_limited(reader)?;
    let mut parts = line.split_whitespace();
    let method = Method::parse(parts.next().ok_or(ParseError::BadRequest)?)
        .ok_or(ParseError::MethodNotAllowed)?;
    let path = parts.next().ok_or(ParseError::BadRequest)?.to_string();
    let version = parts.next().ok_or(ParseError::BadRequest)?.to_string();

    let mut header = HashMap::new();
    loop {
        let line = read_line_limited(reader)?;
        if line.is_empty() { break; }
        if header.len() >= MAX_HEADERS { return Err(ParseError::BadRequest); }
        let (k, v) = line.split_once(':').ok_or(ParseError::BadRequest)?;
        header.insert(k.trim().to_lowercase(), v.trim().to_string());
    }

    let len = match header.get("content-length") {
        Some(v) => v.parse::<usize>().map_err(|_| ParseError::BadRequest)?,
        None => 0,
    };
    if len > MAX_BODY { return Err(ParseError::TooLarge); }

    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).map_err(|_| ParseError::BadRequest)?;
    Ok(Request { method, path, version, header, body })
}

//Calls action based on request type and path
pub fn handle_request_dispatch(request: &Request, table: &HashMap<(Method, &'static str), fn(&Request) -> Response>) -> Response {

    //Check if in dispatch table
    if let Some(handler) = table.get(&(request.method, request.path.as_str())) {
        return handler(request)
    }
    //Check if its requesting static file
    if request.method == Method::GET && request.path.starts_with("/files/") {
        return file_handling(request)
    }
    if request.method == Method::POST && (request.path == "/upload" ||request.path.starts_with("/upload/")) {
        return post_upload(request);
    }
    //Check if echoing a string
    if request.method == Method::GET && (request.path == "/echo" || request.path.starts_with("/echo/")) {
        return echo_handling(request);
    }
    
    //If none of the above then error 404
    println!("Handle request default");
    default_handler(request)
}

//Serves a file from root
fn file_handling(req: &Request) -> Response {
    
    //Get file name after /
    let file = &req.path["/files/".len()..];
    
    //Sanitize for .. or absolute paths

    let full_path = Path::new("root").join(file);

    let Some(full_path) = safe_path(file) else { return default_handler(req); };

    match fs::read(&full_path) {
        Ok(body) => {
            //Check keep connection alive
            let close_conn = req.header.get("connection").map(|val| val == "close").unwrap_or(false);
            let con_header = if close_conn { "close" } else { "keep-alive" };
            let cont_type = content_type(&full_path);
            let zip = req.header.get("accept-encoding")
            .map(|val| val.split(',').any(|enc| enc.trim().eq_ignore_ascii_case("gzip")))
            .unwrap_or(false);

            let (body, gzipped) = if zip {
                match gzip_encoding(&body) {
                    Ok(compressed) => (compressed, true),
                    Err(_) => (body, false),
                }
            } else {
                (body, false)
            };
            
            //Make header
            let mut header: HashMap<&'static str, String> = [
                ("Content-length", body.len().to_string()),
                ("Connection", con_header.to_string()),
                ("Content-type", cont_type.to_string()),
            ].into_iter().collect();

            if gzipped {
                header.insert("Content-Encoding", "gzip".to_string());
            }


            //Return the response
            Response { status: Status::Ok, header, body}
        }
        Err(_) => { 
            println!("Match fs default {}", full_path.display());
            default_handler(req) 
        }
    }
}

fn post_upload(req: &Request) -> Response {

    let Some(name) = req.path.strip_prefix("/upload/") else {
        return error_response(req, Status::BadRequest);
    };

    if name.is_empty() || name.contains(['/', '\\']) || name == ".." || name == "." {
        return error_response(req, Status::BadRequest);
    }

    let Ok(root) = Path::new("root").canonicalize() else {
        return error_response(req, Status::InternalServerError);
    };

    let full_path = root.join(name);
    let (file, status) = match OpenOptions::new().write(true).create_new(true).open(&full_path) {
        Ok(f) => (Ok(f), Status::Created),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => (
            OpenOptions::new().write(true).truncate(true).open(&full_path),
            Status::Ok,
        ),
        Err(e) => (Err(e), Status::Ok),
    };

    match file.and_then(|mut f| f.write_all(&req.body)) {
        Ok(()) => {
            //Check keep connection alive
            let close_conn = req.header.get("connection").map(|val| val == "close").unwrap_or(false);
            let con_header = if close_conn { "close" } else { "keep-alive" };
            
            //Make header
            let header = [
                ("Content-length", "0".to_string()),
                ("Connection", con_header.to_string()),
            ].into_iter().collect();

            //Return the response
            Response { status, header, body: Vec::new()}
        }
        Err(_) => { 
            println!("Match fs default {}", full_path.display());
            default_handler(req)
        }

    }

}

fn echo_handling(req: &Request) -> Response {
    let text = req.path.strip_prefix("/echo/").unwrap_or("");
    let body: Vec<u8> = text.as_bytes().to_vec();
    
    let close_conn = req.header.get("connection").map(|val| val == "close").unwrap_or(false);
    let con_header = if close_conn { "close" } else { "keep-alive" };
    let zip = req.header.get("accept-encoding")
    .map(|val| val.split(',').any(|enc| enc.trim().eq_ignore_ascii_case("gzip")))
    .unwrap_or(false);

    let (body, gzipped) = if zip {
        match gzip_encoding(&body) {
            Ok(compressed) => (compressed, true),
            Err(_) => (body, false),
        }
    } else {
        (body, false)
    };

    let cont_type = "text/plain";
    let body_len = body.len();
    let mut header: HashMap<&'static str, String> = [
        ("Content-length", body_len.to_string()),
        ("Connection", con_header.to_string()),
        ("Content-type", cont_type.to_string()),
    ].into_iter().collect();

    if gzipped {
        header.insert("Content-Encoding", "gzip".to_string());
    }

    Response {status: Status::Ok, header, body}

}

//Just prints all incoming request
pub fn read_request<r: BufRead>(reader: r){
    //read entire request
    let request: Vec<_> = reader.lines()
        .map(|res| res.unwrap())
        .take_while(|line| !line.is_empty())
        .collect();

    for line in &request{
        println!("{}", line);
    }

}

//Provides the content type of the file at given file path
fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html",
        Some("css") => "text/css",
        Some("js") => "application/javascript",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("txt") => "text/plain",
        _ => "application/octet-stream",
    }
}

pub fn get_handle_root(req: &Request) -> Response {
    
    let html = fs::read_to_string("root/page.html").expect("Failed to read page.html");

    let close_conn = req.header.get("connection").map(|val| val == "close").unwrap_or(false);
    let con_header = if close_conn { "close" } else { "keep-alive" }; 

    let header: HashMap<&'static str, String> = [
        ("Content-length", html.len().to_string()),
        ("Connection", con_header.to_string()),
    ]
        .into_iter()
        .collect();

    Response { status: Status::Ok, header, body: html.into_bytes()}
}

fn gzip_encoding(body: &[u8]) -> std::io::Result<Vec<u8>> {

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&body)?;
    encoder.finish()
}

fn safe_path(rel: &str) -> Option<PathBuf> {
    let root = Path::new("root").canonicalize().ok()?;
    let full = root.join(rel).canonicalize().ok()?;  // resolves .. and symlinks
    full.starts_with(&root).then_some(full)
}

fn is_close(req: &Request) -> bool {
    req.header
        .get("connection")
        .is_some_and(|v| v.eq_ignore_ascii_case("close"))
}

fn build_response(req: &Request, status: Status, body: Vec<u8>, ctype: Option<&'static str>, allow_gzip: bool) -> Response {
    let wants_gzip = allow_gzip
        && req
            .header
            .get("accept-encoding")
            .is_some_and(|v| v.split(',').any(|e| e.trim().eq_ignore_ascii_case("gzip")));

    let (body, gzipped) = if wants_gzip {
        match gzip_encoding(&body) {
            Ok(c) => (c, true),
            Err(_) => (body, false),
        }
    } else {
        (body, false)
    };

    let mut header = HashMap::new();
    header.insert("Content-Length", body.len().to_string());
    header.insert(
        "Connection",
        if is_close(req) { "close" } else { "keep-alive" }.to_string(),
    );
    if let Some(ct) = ctype {
        header.insert("Content-Type", ct.to_string());
    }
    if gzipped {
        header.insert("Content-Encoding", "gzip".to_string());
    }
    Response { status, header, body }
}

fn error_response(req: &Request, status: Status) -> Response {
    let body = format!("{} {}\n", status.status_num(), status.reason()).into_bytes();
    build_response(req, status, body, Some("text/plain"), false)
}

// For parse errors, where there's no Request yet
fn error_bytes(status: Status) -> Vec<u8> {
    let body = format!("{} {}\n", status.status_num(), status.reason());
    format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n{}",
        status.status_num(),
        status.reason(),
        body.len(),
        body
    )
    .into_bytes()
}

pub fn default_handler(req: &Request) -> Response {
    let html = fs::read_to_string("root/error.html").expect("Failed to read page.html");

    let close_conn = req.header.get("connection").map(|val| val == "close").unwrap_or(false);
    let con_header = if close_conn { "close" } else { "keep-alive" }; 

    let header: HashMap<&'static str, String> = [
        ("Content-length", html.len().to_string()),
        ("Connection", con_header.to_string()),
    ]
        .into_iter()
        .collect();

    Response { status: Status::NotFound, header, body: html.into_bytes()}

}
