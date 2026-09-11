use std::thread::{self, JoinHandle, ThreadId};
use std::net::{TcpStream};
use crate::handle_request;
use std::sync::mpsc::{self, Sender, Receiver};
use std::sync::{Arc, Mutex};



pub struct ThreadPool{
    //Have a vector to hold worker
    pool: Vec<T_Worker>,
    //Sender end of channel to send TcpStream objects
    sender: Option<Sender<TcpStream>>,
    //Size of vector cause I guess
}

pub struct T_Worker {
    //Worker var to hold the thread
    worker: JoinHandle<()>,
    //Id of thread cause why not
    id: ThreadId,
}

//Thread worker functions
impl T_Worker {
    //Pass copy of receiver to thread that way the can listen for job
    pub fn new(rec: Arc<Mutex<Receiver<TcpStream>>>) -> Self {
        
        //Spawn thread have it constantly look for a job using receiver
        let t = thread::spawn(move || {
            loop {
                let job = rec.lock().unwrap().recv();

                match job {

                    Ok(job) => { let _ = handle_request(job); },
                    Err(_) => break,
                }

            }
        });

        //Intialize and return worker
        Self {
            id: t.thread().id(),
            worker: t,
        }
    }
}

//Thread pool functions
impl ThreadPool {
    
    //Make thread pool instance
    pub fn build(t_num: usize) -> Self{
        //Make sure not zero threads small amount
        if t_num == 0 || t_num > 10 {
            panic!("Cannot have that number of threads");
        }
        
        //Make receiver and sender
        let (s, r) = mpsc::channel(); 
        let receiver = Arc::new(Mutex::new(r));        
        //Make and return instance of thread pool
        Self {
            pool: (0..t_num).map(|_| T_Worker::new(Arc::clone(&receiver))).collect(),
            sender: Some(s),
        }

    }
    
    //Send TcpStream object down channel for threads
    pub fn execute(&self, stream: TcpStream) {
        self.sender.as_ref().unwrap().send(stream).unwrap();
    }

}

impl Drop for ThreadPool {

    fn drop(&mut self) {
        
        if let Some(sender) = self.sender.take() {
            drop(sender);
        };

        for thr in self.pool.drain(..) {
            thr.worker.join().unwrap();
        }
    }
}
