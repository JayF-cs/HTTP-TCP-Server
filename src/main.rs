use std::io::{Read, Write, BufReader, BufRead};
use std::net::{TcpListener, TcpStream};

fn main() -> Result<(), Box<dyn std::error::Error>>{
    
    //Make listener on port 8080
    let listener: TcpListener = TcpListener::bind("127.0.0.1:8080").unwrap();
    
    for stream in listener.incoming(){
        //Get TcpStream instance
        let mut stream = stream.unwrap();
        
        //Make a buffer read instance
        let reader = BufReader::new(&mut stream);
        
        //Get start line
        let request: String = reader.lines().next().unwrap().unwrap();
        
        //Match request type
        let message: String = match &request[..]{
            "GET / HTTP/1.1" => format!("HTTP/1.1 200 OK\r\n\r\n"),
            _ => format!("HTTP/1.1 404 NOT FOUND\r\n\r\n"),
        };

        stream.write_all(&message.into_bytes())?;

    }

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
