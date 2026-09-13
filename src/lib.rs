pub mod thread_pool;
use std::collections::HashMap;
use std::net::{TcpStream};
use std::io::{Write, BufReader, BufRead};
use std::fs;

pub fn handle_request(mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error>>{
    
    let mut writer = stream.try_clone()?;
    //Create reader to read the buffer stream
    let mut reader = BufReader::new(&mut stream);
    loop {
        //Get start line
        let request: String = match (&mut reader).lines().next() {
            Some(Ok(line)) => line,
            Some(Err(e)) => {
                eprintln!("Can't read request line");
                break;
            }
            None => break,

        };

        //Parse header section into hashmap
        let header: HashMap<String, String> = (&mut reader).lines().map(|section| section.unwrap()).map_while(|line| {
            if line.is_empty() {
                None
            } else {
                line.split_once(':').map(|(k,v)| {(k.to_string().to_lowercase(), v.trim().to_string())})
            }
        })
            .collect();

        println!("{:#?}", header);

        let start_line;
        let cont_len: usize;
        let html: String;
        
        //Match request type
        match &request[..] {
            "GET / HTTP/1.1" => {
                start_line = "HTTP/1.1 200 OK";
                html = fs::read_to_string("page.html").expect("Failed to read page.html");
                cont_len = html.len();
            },
            _ => {
                start_line = "HTTP/1.1 404 NOT FOUND";
                html = fs::read_to_string("error.html").expect("Failed to read error.html");
                cont_len = html.len();
            }
        }
        
        let close_conn = header.get("connection").map(|val| val == "close").unwrap_or(false);

        let con_header = if close_conn { "close" } else { "keep-alive" }; 

        //Make message
        let message = format!("{start_line}\r\nContent-Length: {cont_len}\r\nConnection: {con_header}\r\n\r\n{html}");

        //Write back message
        writer.write_all(&message.into_bytes())?;

        //Check if keep connection open
        if close_conn {
            break;
        }
    }

    Ok(())
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
