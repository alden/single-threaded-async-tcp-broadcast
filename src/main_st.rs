use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::rc::Rc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    // basic logging with thread ids & names. Use RUST_LOG=info to adjust displayed levels.
    tracing_subscriber::fmt()
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // TODO: proper arg parsing. Currently the only arg is port number.
    let port = match std::env::args().nth(1) {
        None => 58888,
        Some(port) => port.parse::<u16>().expect("Could not parse port."),
    };

    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    println!("listening on port {}", port);

    // Non-thread safe HashMap to store all connected clients
    let clients: Rc<RefCell<HashMap<SocketAddr, UnboundedSender<String>>>> =
        Rc::new(RefCell::new(HashMap::new()));

    let local = task::LocalSet::new();

    local
        .run_until(async move {
            loop {
                let (stream, addr) = listener.accept().await.unwrap();
                info!("accepted new client {:?}", &addr);

                let (read, mut write) = stream.into_split();

                let (tx, mut rx) = mpsc::unbounded_channel::<String>();
                clients.borrow_mut().insert(addr, tx);

                // Rx consumer task
                task::spawn_local(async move {
                    while let Some(msg) = rx.recv().await {
                        if let Err(e) = write.write_all(msg.as_ref()).await {
                            error!("Error writing to output stream: {}", e);
                            break;
                        }
                    }
                });

                // Tx write task
                let clients = clients.clone();
                task::spawn_local(async move {
                    if let Err(e) = clients
                        .borrow()
                        .get(&addr)
                        .unwrap()
                        .send(format!("LOGIN:{}\n", addr.port()))
                    {
                        error!("Could not send login ack message {}. Exiting task.", e);
                        return;
                    }

                    let reader = tokio::io::BufReader::new(read);

                    let mut read = reader.lines();
                    while let Ok(Some(line)) = read.next_line().await {
                        let msg = format!("MESSAGE:{} {}\n", addr.port(), line);
                        info!("read {:?}. Total clients: {}", &msg, clients.borrow().len());

                        // write ack
                        if let Err(e) = clients
                            .borrow()
                            .get(&addr)
                            .unwrap()
                            .send("ACK:MESSAGE\n".to_string())
                        {
                            error!("Could not send ack: {}. Exiting task.", e);
                            break;
                        };

                        // broadcast to remaining clients
                        for (other_addr, wtr) in clients.borrow().iter() {
                            if *other_addr != addr {
                                if let Err(e) = wtr.send(msg.clone()) {
                                    warn!("Could not send message to {}: {}", other_addr, e)
                                }
                            }
                        }
                    }
                    // remove from clients map on disconnect.
                    clients.borrow_mut().remove(&addr);
                    info!("client {:?} disconnected", &addr);
                });
            }
        })
        .await;
    Ok(())
}
