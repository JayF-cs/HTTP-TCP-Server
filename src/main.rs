use ctrlc;
use std::net::{TcpListener};
use http_server::thread_pool::ThreadPool;
use http_server::{route_table};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use std::thread;

fn main() -> Result<(), Box<dyn std::error::Error>>{
   
    //Create shutdown flag with AtomicBool for thread saftey
    let running = Arc::new(AtomicBool::new(true));

    //Clone will be given to ctrlc so main thread maintains ownership of value
    let r = running.clone();
    
    //Set handler for Ctrl + c
    ctrlc::set_handler(move || {r.store(false, Ordering::SeqCst);})?;

    //Make listener on port 8080
    let listener: TcpListener = TcpListener::bind("127.0.0.1:8080").unwrap();
    listener.set_nonblocking(true).expect("Cannot set non-blocking");

    //Make dispatch table
    let routes = route_table();
    let rt = Arc::clone(&routes);

    //Make threadpool
    let tp: ThreadPool = ThreadPool::build(4, rt);

    //Go through all incoming streams
    for stream in listener.incoming(){
        //Check if running flag is still set, if not break out loop
        if !running.load(Ordering::SeqCst) {
            break;
        }

        //Check if stream is valid or if still waiting for a request to come through
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

