# http_server

A multithreaded HTTP/1.1 server written in Rust from scratch, using only the standard library for networking (no web framework). I built it to learn systems-level concurrency and to understand what makes network-facing code safe or unsafe.

## Features

- Worker thread pool (`Arc<Mutex<Receiver>>` over an `mpsc` channel) with a configurable number of workers
- Graceful shutdown on Ctrl+C: stops accepting connections, then drains and joins all workers
- Panic isolation: a panicking handler doesn't take down its worker thread
- HTTP/1.1 keep-alive and `Connection: close` handling
- Routing through a dispatch table keyed by `(Method, path)`, plus prefix routes
- Static file serving from `root/` with content-type detection
- File upload via POST
- Optional gzip compression (when the client sends `Accept-Encoding: gzip`)
- Proper error responses (400, 404, 405, 413, 500)

## Endpoints

| Method | Path             | Description                                  |
|--------|------------------|----------------------------------------------|
| GET    | `/`, `/about`    | Serves `root/page.html`                      |
| GET    | `/files/<path>`  | Serves a file from `root/`                   |
| GET    | `/echo/<text>`   | Returns `<text>` as plain text               |
| POST   | `/upload/<name>` | Writes the request body to `root/<name>`     |


## Design decisions

- **Thread pool over thread-per-connection:** bounds resource use and avoids unbounded thread creation under load.
- **Polling accept loop:** the listener is non-blocking with a short sleep so the main loop can check the shutdown flag. This adds up to ~100ms of accept latency when idle. An event loop (`mio`/`tokio`) would remove it, but I chose simplicity here.
- **Close after parse errors:** once a request is malformed, the framing of the rest of the stream can't be trusted, so the server responds and closes the connection.
- **Function-pointer dispatch table:** handlers are plain `fn(&Request) -> Response`, which are cheap to share across threads via `Arc`.

Known limitations:

- No authentication; anyone can upload to `/upload/`, and uploaded files are served back
- No TLS
- `Transfer-Encoding: chunked` is not supported
- Timeouts apply per read, not per request, so a slow-drip client can still hold a worker
- No percent-decoding of paths or query-string handling
- Thread-per-connection model means idle keep-alive connections occupy workers

## Building and running

Requires a recent stable Rust toolchain.

```
cargo build --release
cargo run --release
```

The server listens on `127.0.0.1:8080` and serves files from the `root/` directory (must contain `page.html` and `error.html`).

```
curl http://127.0.0.1:8080/
curl http://127.0.0.1:8080/echo/hello
curl -H "Accept-Encoding: gzip" --compressed http://127.0.0.1:8080/echo/hello
curl -X POST --data-binary @notes.txt http://127.0.0.1:8080/upload/notes.txt
curl http://127.0.0.1:8080/files/notes.txt
```

Stop with Ctrl+C for a graceful shutdown.

## Project layout

```
src/main.rs         Accept loop, signal handling, pool setup
src/lib.rs          HTTP parsing, routing, handlers, responses
src/thread_pool.rs  Worker pool and shutdown logic
root/               Static files served by the server
```
