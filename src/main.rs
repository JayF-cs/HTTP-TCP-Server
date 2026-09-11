use ctrlc;
use std::net::{TcpListener};
use http_server::{thread_pool::ThreadPool};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::thread;

fn main() -> Result<(), Box<dyn std::error::Error>>{
   
    //Create shutdown flag with AtomicBool for thread saftey
    let running = Arc::new(AtomicBool::new(true));

    //Clone will be given to ctrlc so main thread maintains ownership of value
    let r = running.clone();

    ctrlc::set_handler(move || {r.store(false, Ordering::SeqCst);})?;

    //Make listener on port 8080
    let listener: TcpListener = TcpListener::bind("127.0.0.1:8080").unwrap();
    listener.set_nonblocking(true).expect("Cannot set non-blocking");


    let tp: ThreadPool = ThreadPool::build(4);
    
    for stream in listener.incoming(){
        //Get TcpStream instance
        if !running.load(Ordering::SeqCst) {
            break;
        }

        match stream {
            Ok(s) => {
                //Handle the request
                tp.execute(s);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) => {
                panic!("{e}");
            }
        }
    }
        
    Ok(())
}

