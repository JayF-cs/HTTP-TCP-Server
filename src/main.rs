use std::net::{TcpListener, TcpStream};
use http_server::{handle_request, thread_pool::ThreadPool};

fn main() -> Result<(), Box<dyn std::error::Error>>{
    
    //Make listener on port 8080
    let listener: TcpListener = TcpListener::bind("127.0.0.1:8080").unwrap();
    let tp: ThreadPool = ThreadPool::build(4);
    
    for stream in listener.incoming(){
        //Get TcpStream instance
        let mut stream = stream.unwrap();

        //Handle the request
        tp.execute(stream);
    }

    Ok(())
}

