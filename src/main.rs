use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // basic logging with thread ids & names. Use RUST_LOG=info to adjust displayed levels.
    tracing_subscriber::fmt()
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    // TODO: proper arg parsing. Currently the only arg is port number.
    let port = match std::env::args().nth(1) {
        None => 8888,
        Some(port) => port.parse::<u16>().expect("Could not parse port."),
    };

    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    println!("listening on port {}", port);

    // thread safe shared map to store all connected clients
    let clients: Arc<RwLock<HashMap<SocketAddr, OwnedWriteHalf>>> =
        Arc::new(RwLock::new(HashMap::new()));

    loop {
        let (stream, addr) = listener.accept().await?;
        info!("accepted new client {:?}", &addr);

        let clients = Arc::clone(&clients);
        tokio::spawn(async move {
            if let Err(e) = process(stream, addr, clients).await {
                error!("Error in task for {}: {}", addr, e);
            }
        });
    }
}

async fn process(
    stream: TcpStream,
    addr: SocketAddr,
    clients: Arc<RwLock<HashMap<SocketAddr, OwnedWriteHalf>>>,
) -> Result<(), Box<dyn Error>> {
    let (read, write) = stream.into_split();
    clients.write().await.insert(addr, write);

    clients
        .write()
        .await
        .get_mut(&addr)
        .ok_or("Invalid state. Addr is not in clients map.")?
        .write_all(format!("LOGIN: {}\n", addr.port()).as_ref())
        .await?;

    let reader = tokio::io::BufReader::new(read);

    let mut read = reader.lines();
    while let Ok(Some(line)) = read.next_line().await {
        let msg = format!("MESSAGE:{} {}\n", addr.port(), line);
        info!(
            "read {:?}. Total clients: {}",
            msg,
            clients.read().await.len()
        );

        // write ack
        clients
            .write()
            .await
            .get_mut(&addr)
            .ok_or("Invalid state. Addr is not in clients map.")?
            .write_all("ACK:MESSAGE\n".as_ref())
            .await?;

        // broadcast to remaining clients
        for (other_addr, wtr) in clients.write().await.iter_mut() {
            if *other_addr != addr {
                wtr.write_all(msg.as_ref()).await?;
            }
        }
    }

    // remove from clients map on disconnect
    clients
        .write()
        .await
        .remove(&addr)
        .ok_or("Invalid state. Addr is not present in clients map.")?;
    info!("client {:?} disconnected", &addr);
    Ok(())
}
