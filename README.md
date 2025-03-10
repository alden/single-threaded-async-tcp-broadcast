This project contains two implementations of an asynchronous TCP broadcast server:

#### Implementation 1: Uses Arc & RwLock synchronization. Set to use Tokio's `current_thread` runtime, but also supports the default `multi_thread`.

```bash
RUST_LOG=info cargo run  # default port number 8888
# RUST_LOG=info cargo run -- 58888  # custom port number 58888

# In new terminal sessions:
nc localhost 8888
```

#### Implementation 2: Fully single threaded: Uses Tokio's LocalSet and mpsc channels.
```bash
RUST_LOG=info cargo run --bin server_st  # default port number 8888.
# RUST_LOG=info cargo run --bin server_st -- 58888  # custom port number 58888

# In new terminal sessions:
nc localhost 8888
```
