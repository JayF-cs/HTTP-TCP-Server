pub mod thread_pool;
use std::collections::HashMap;
use std::net::{TcpStream};
use std::io::{Write, BufReader, BufRead};
use std::sync::Arc;
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
}

impl Status {
    fn status_num(&self) -> u16 {
        match self {
            Status::Ok => 200,
            Status::NotFound => 404,
        }
    }

    fn reason(&self) -> &'static str {
        match self {
            Status::Ok => "OK",
            Status::NotFound => "Not Found",
        }
    }
}

pub struct Request {
    method: Method,
    path: String,
    pub version: String,
    pub header: HashMap<String, String>,
    body: Vec<u8>,
}

pub struct Response {
    pub status: Status,
    pub header: HashMap<&'static str, String>,
    pub body: Vec<u8>,
}

pub fn route_table() -> Arc<HashMap<(Method, &'static str), fn(&Request) -> Response>> {

    let table: HashMap<(Method, &str), fn(&Request) -> Response> = [
        ((Method::GET, "/"), get_handle_root as fn(&Request) -> Response),
        ((Method::GET, "/about"), get_handle_root),
    ]
        .into_iter()
        .collect();

    Arc::new(table)
}

pub fn handle_request(mut stream: TcpStream, table: Arc<HashMap<(Method, &'static str), fn(&Request) -> Response>>) -> Result<(), Box<dyn std::error::Error>> {
    
    let mut writer = stream.try_clone()?;
    //Create reader to read the buffer stream
    let mut reader = BufReader::new(&mut stream);
    loop {
        
        let request = match parse_request(&mut reader) {
            Some(req) => req,
            None => break,
        };

        let response = match table.get(&(request.method, request.path.as_str())) {
            
            Some(handler) => handler(&request),
            None => default_handler(&request),
        };
        
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

pub fn parse_request<r: BufRead>(reader: &mut r) -> Option<Request> {
    let request: (Method, String, String) = match reader.lines().next() {
        Some(Ok(line)) => {
            let mut parts = line.split_whitespace();
            let req_type = Method::parse(parts.next().unwrap_or("")).unwrap();
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
    
    Some(Request {method: request.0, path: request.1, version: request.2, header, body: Vec::new()})

}

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

pub fn get_handle_root(req: &Request) -> Response {
    
    let html = fs::read_to_string("page.html").expect("Failed to read page.html");

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
    let html = fs::read_to_string("error.html").expect("Failed to read page.html");

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
