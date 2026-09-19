pub mod thread_pool;
use std::collections::HashMap;
use std::net::{TcpStream};
use std::io::{Write, BufReader, BufRead, Read};
use std::sync::Arc;
use std::path::Path;
use std::fs;

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

pub enum Status {
    Ok,
    NotFound,
    Created,
}

impl Status {
    fn status_num(&self) -> u16 {
        match self {
            Status::Ok => 200,
            Status::NotFound => 404,
            Status::Created => 201,
        }
    }

    fn reason(&self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::NotFound => "Not Found",
            Status::Created => "Created",
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
    
    let mut writer = stream.try_clone()?;
    //Create reader to read the buffer stream
    let mut reader = BufReader::new(&mut stream);
    loop {
        
        let request = match parse_request(&mut reader) {
            Some(req) => req,
            None => break,
        };

        let response = handle_request_dispatch(&request, &table);
        
        let start_line = format!("{} {} {}",request.version, response.status.status_num(), response.status.reason());

        let header = response.header
            .iter()
            .map(|(k,v)| format!("{k}: {v}"))
            .collect::<Vec<String>>()
            .join("\r\n");
        
        //Make message
        let partial_message = format!("{start_line}\r\n{header}\r\n\r\n");
        let mut message = partial_message.into_bytes();
        message.extend_from_slice(&response.body);
        //Write back message
        writer.write_all(&message)?;

        //Check if keep connection open
        if request.header.get("connection").map(|val| val == "close").unwrap_or(false) {
            break;
        }
    }

    Ok(())
}

//Parse request into different sections
pub fn parse_request<r: BufRead>(reader: &mut r) -> Option<Request> {
    //Parse request line into different sections
    let request: (Method, String, String) = match reader.lines().next() {
        Some(Ok(line)) => {
            let mut parts = line.split_whitespace();
            let req_type = match Method::parse(parts.next().unwrap_or("")) {
                Some(m) => m,
                None => return None,
            };
            let path = parts.next().unwrap_or("").to_string();
            let ver = parts.next().unwrap_or("").to_string();
            (req_type, path, ver)
        }
        Some(Err(e)) => return None,
        None => return None,

    };

    //parse header section into hashmap
    let header: HashMap<String, String> = reader.lines().map(|section| section.unwrap()).map_while(|line| {
        if line.is_empty() {
            None
        } else {
            line.split_once(':').map(|(k,v)| {(k.to_string().to_lowercase(), v.trim().to_string())})
        }
    })
        .collect();
    
    //Read the body
    //Check if content length is in header, then either get the length or fall to _ case
    //If the length is 0 also fall through to _ case
    //Otherwise create of Vec<u8> of size content length
    //Use reader to read into the buffer and return that
    let body = match header.get("content-length").and_then(|v| v.parse::<usize>().ok()) {
        Some(len) if len > 0 => {
            let mut buf = vec![0u8; len];
            reader.read_exact(&mut buf);
            buf
        }
        _ => Vec::new(),
    };
        
    //if request.0 != Method::PUT {
    Some(Request {method: request.0, path: request.1, version: request.2, header, body})
    //} else {
        //Not implemented yet
    //}
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
    
    //If none of the above then error 404
    println!("Handle request default");
    default_handler(request)
}

//Serves a file from root
fn file_handling(req: &Request) -> Response {
    
    //Get file name after /
    let file = &req.path["/files/".len()..];
    
    //Sanitize for .. or absolute paths
    if file.contains("..") || file.starts_with("/") {
        return default_handler(req);
    }

    let full_path = Path::new("root").join(file);

    match fs::read(&full_path) {
        Ok(body) => {
            //Check keep connection alive
            let close_conn = req.header.get("connection").map(|val| val == "close").unwrap_or(false);
            let con_header = if close_conn { "close" } else { "keep-alive" };
            let cont_type = content_type(&full_path);

            //Make header
            let header = [
                ("Content-length", body.len().to_string()),
                ("Connection", con_header.to_string()),
                ("Content-type", cont_type.to_string()),
            ].into_iter().collect();

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

    let Some(file) = extract_filename(req) else {
        return default_handler(req);
    };

    if file.is_empty() || file.contains("..") || file.starts_with("/") {
        return default_handler(req);
    }

    let full_path = Path::new("root").join(file);

    let status = if full_path.exists() {
        Status::Ok
    } else {
        Status::Created
    };

    match fs::write(&full_path, &req.body) {
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

//Extract file path or just return the name in the header section
fn extract_filename(req: &Request) -> Option<String> {
    if let Some(res) = req.path.strip_prefix("/upload/") {
        return Some(res.to_string())
    }

    req.header.get("x-filename").cloned()
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
