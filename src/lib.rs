pub mod thread_pool;
use std::collections::HashMap;
use std::net::{TcpStream};
use std::io::{Write, BufReader, BufRead};
use std::fs;

pub fn handle_request(mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error>>{
    
    //Create reader to read the buffer stream
    let mut reader = BufReader::new(&mut stream);
    
    //Get start line
    let request: String = (&mut reader).lines().next().unwrap().unwrap();

    //Parse header section into hashmap
    let header: HashMap<String, String> = (&mut reader).lines().map(|section| section.unwrap()).map_while(|line| {
        if line.is_empty() {
            None
        } else {
            let sect = line.split_once(':').unwrap();
            let pairs = {
                let (k, v) = sect;
                (k.to_string().to_lowercase(), v.trim().to_string()) //Make sure to lowercase header
                //section as it is case insensitive as well as trim whitespace from value
            };

            Some(pairs)
        }
    })
        .collect();
    
    println!("{:#?}", header);

    let start_line;
    let cont_len: usize;
    let html: String;

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

    let message = format!("{start_line}\r\nContent-Length: {cont_len}\r\nConnection: close\r\n\r\n{html}");


    stream.write_all(&message.into_bytes())?;

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
