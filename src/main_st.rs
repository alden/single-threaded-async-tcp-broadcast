use futures::future::LocalBoxFuture;
use futures::stream::FuturesUnordered;
use futures::{FutureExt, StreamExt, select};
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tracing::info;
use tracing_subscriber::EnvFilter;

struct Client {
    id: SocketAddr,
    reader: tokio::io::Lines<BufReader<OwnedReadHalf>>,
}

impl Client {
    async fn read_line(mut self) -> (Option<String>, Client) {
        let result = self.reader.next_line().await;
        (result.ok().flatten(), self)
    }
}

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
        None => 8888,
        Some(port) => port.parse::<u16>().expect("Could not parse port."),
    };

    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    println!("listening on port {}", port);

    // Non-thread safe HashMap to store all connected clients
    let mut clients: HashMap<SocketAddr, OwnedWriteHalf> = HashMap::new();

    let mut futures: FuturesUnordered<LocalBoxFuture<(Option<String>, Client)>> =
        FuturesUnordered::new();

    loop {
        select! {
            login = Box::pin(listener.accept().fuse()) => {
                let (stream, addr) = login.unwrap();
                info!("accepted new client {:?}", &addr);

                let (read, mut write) = stream.into_split();

                let _ = write.write_all(format!("LOGIN:{}\n", addr.port()).as_ref()).await;
                clients.insert(addr, write);

                let reader = tokio::io::BufReader::new(read);

                let read = reader.lines();
                let client = Client { id: addr, reader: read };

                futures.push(Box::pin(client.read_line()));
            }
            read_line = futures.next() => {
                match read_line {
                    None => {
                        info!("FuturesUnordered empty");
                    }  // empty FuturesUnordered, TODO: docs say to not re-poll
                    Some(read_line) => {
                        match read_line {
                            (None, client) => {
                                clients.remove(&client.id);
                                info!("{:} disconnected", &client.id);
                            }
                            (Some(msg), client) => {
                                info!("msg");
                                for (other_addr, wtr) in clients.iter_mut() {
                                    if *other_addr != client.id {
                                        let _ = wtr.write_all(format!("MESSAGE:{} ", client.id.port()).as_ref()).await;
                                        let _ = wtr.write_all(msg.as_ref()).await;
                                        let _ = wtr.write_all("\n".as_bytes()).await;
                                    }
                                    else {
                                        let _ = wtr.write_all("ACK:MESSAGE\n".as_ref()).await;
                                    }
                                }

                                futures.push(Box::pin(client.read_line()));
                            }
                        }
                    }
                }
            }
        }
    }
}
