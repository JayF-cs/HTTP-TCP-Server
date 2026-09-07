use std::fs;
use std::io::{Read, Write, BufReader, BufRead};
use std::net::{TcpListener, TcpStream};

fn main() -> Result<(), Box<dyn std::error::Error>>{
    
    //Make listener on port 8080
    let listener: TcpListener = TcpListener::bind("127.0.0.1:8080").unwrap();
    
    for stream in listener.incoming(){
        //Get TcpStream instance
        let mut stream = stream.unwrap();

        //Handle the request
        handle_request(stream);        
    }

    Ok(())
}

fn handle_request(mut stream: TcpStream) -> Result<(), Box<dyn std::error::Error>>{
    
    //Create reader to read the buffer stream
    let reader = BufReader::new(&mut stream);
    
    //Get start line
    let request: String = reader.lines().next().unwrap().unwrap();

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

    let message = format!("{start_line}\r\nContent-Length: {cont_len}\r\n\r\n{html}");


    stream.write_all(&message.into_bytes())?;

    Ok(())
}

fn read_request<R: BufRead>(reader: R){
    //Read entire request
    let request: Vec<_> = reader.lines()
        .map(|res| res.unwrap())
        .take_while(|line| !line.is_empty())
        .collect();

    for line in &request{
        println!("{}", line);
    }

}
