# stream-media

A microservices-based media streaming platform built in Rust. Serves both video and audio with HTTP Range request support for seeking. Each service is independently deployable and swappable — you can change the framework, database, or streaming implementation in one service without affecting the others.

## Architecture

```
Client
  │
  ▼
Gateway (:3000)
  ├── /api/media/*     → Catalog Service (:3001)
  ├── /stream/*        → Streaming Service (:3002)
  └── /upload          → Streaming Service (:3002)
```

| Crate | Port | Purpose |
|-------|------|---------|
| `gateway` | 3000 | Reverse proxy, CORS, single entry point for clients |
| `catalog-service` | 3001 | Media metadata CRUD backed by SQLite3 |
| `streaming-service` | 3002 | File upload and streaming with HTTP Range support |
| `common` | — | Shared types, error handling, configuration |

Services communicate over HTTP, making each one independently restartable, replaceable, and scalable.

## API

All endpoints are accessed through the gateway on port 3000.

### Media Metadata

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/media` | List media. Query params: `search`, `media_type` (video/audio), `limit`, `offset` |
| `POST` | `/api/media` | Create a metadata record (without a file) |
| `GET` | `/api/media/{id}` | Get a single media item |
| `PUT` | `/api/media/{id}` | Update metadata (title, description, duration) |
| `DELETE` | `/api/media/{id}` | Delete a media item |

### Upload and Streaming

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/upload` | Upload a media file (multipart/form-data) |
| `GET` | `/stream/{id}` | Stream a media file. Supports `Range` header for seeking |

### Upload Fields

The `/upload` endpoint accepts `multipart/form-data` with these fields (metadata fields must appear before `file`):

| Field | Required | Description |
|-------|----------|-------------|
| `title` | yes | Media title |
| `media_type` | yes | `video` or `audio` |
| `format` | yes | File extension (e.g. `mp4`, `mp3`, `flac`) |
| `file` | yes | The media file |
| `description` | no | Media description |
| `duration_secs` | no | Duration in seconds |

### Streaming

The `/stream/{id}` endpoint supports standard HTTP Range requests:

- **Full file**: `GET /stream/{id}` returns `200 OK` with `Accept-Ranges: bytes`
- **Partial content**: `GET /stream/{id}` with `Range: bytes=0-1023` returns `206 Partial Content` with `Content-Range` header
- Supported formats include mp4, webm, mkv, mp3, flac, ogg, wav, aac, and others (MIME types are auto-detected)

## Getting Started

### With Podman (recommended)

Build and start all three services in containers:

```bash
podman compose up --build
```

This builds one image per service and runs them on a shared network. The gateway is exposed on port 3000.

To run in the background:

```bash
podman compose up --build -d
```

To stop:

```bash
podman compose down
```

Data is persisted in named volumes (`catalog-data` for the database, `media-data` for uploaded files). To reset all data:

```bash
podman compose down -v
```

### Without containers

Prerequisites: Rust (edition 2024)

```bash
cargo build
```

Start all three services in separate terminals:

```bash
# Terminal 1 — Catalog service (metadata + SQLite)
cargo run -p catalog-service

# Terminal 2 — Streaming service (file upload + streaming)
cargo run -p streaming-service

# Terminal 3 — Gateway (entry point)
cargo run -p gateway
```

### Try It

```bash
# List media (empty at first)
curl http://localhost:3000/api/media

# Upload a file
curl -F "title=My Song" -F "media_type=audio" -F "format=mp3" \
     -F "file=@song.mp3" http://localhost:3000/upload

# Get metadata
curl http://localhost:3000/api/media/{id}

# Stream the full file
curl -o output.mp3 http://localhost:3000/stream/{id}

# Stream with range (for seeking)
curl -H "Range: bytes=0-65535" http://localhost:3000/stream/{id}

# Search by title
curl "http://localhost:3000/api/media?search=song"

# Filter by type
curl "http://localhost:3000/api/media?media_type=audio"

# Update metadata
curl -X PUT -H "Content-Type: application/json" \
     -d '{"title":"Renamed Song"}' \
     http://localhost:3000/api/media/{id}

# Delete
curl -X DELETE http://localhost:3000/api/media/{id}
```

## Configuration

All services read configuration from environment variables with sensible defaults:

| Variable | Default | Description |
|----------|---------|-------------|
| `GATEWAY_PORT` | `3000` | Gateway listen port |
| `CATALOG_PORT` | `3001` | Catalog service listen port |
| `STREAMING_PORT` | `3002` | Streaming service listen port |
| `CATALOG_URL` | `http://127.0.0.1:3001` | URL the gateway/streaming service uses to reach the catalog |
| `STREAMING_URL` | `http://127.0.0.1:3002` | URL the gateway uses to reach the streaming service |
| `DATABASE_PATH` | `./catalog.db` | SQLite database file path |
| `MEDIA_STORE_PATH` | `./media-store` | Directory for uploaded media files |

Set `RUST_LOG` to control log verbosity (e.g. `RUST_LOG=debug`).

## Project Structure

```
stream-media/
├── Cargo.toml                        # Workspace definition
├── Containerfile                     # Multi-stage build (builder + 3 service targets)
├── compose.yaml                      # Podman/Docker compose orchestration
├── common/
│   └── src/
│       ├── lib.rs                    # Module re-exports
│       ├── models.rs                 # MediaItem, MediaType, request/response types
│       ├── error.rs                  # AppError with axum IntoResponse
│       └── config.rs                 # ServiceConfig from environment variables
├── catalog-service/
│   └── src/
│       ├── main.rs                   # Entry point
│       ├── db.rs                     # SQLite repository (schema, CRUD)
│       ├── handlers.rs               # Axum request handlers
│       └── routes.rs                 # Router construction
├── streaming-service/
│   └── src/
│       ├── main.rs                   # Entry point
│       ├── handlers.rs               # Upload (multipart) and stream handlers
│       ├── range.rs                  # HTTP Range header parsing, 206 responses
│       └── routes.rs                 # Router construction
└── gateway/
    └── src/
        ├── main.rs                   # Entry point
        ├── proxy.rs                  # Reverse proxy logic
        └── routes.rs                 # Route mapping and CORS
```

## Tech Stack

- **[axum](https://github.com/tokio-rs/axum)** — HTTP framework
- **[tokio](https://tokio.rs)** — Async runtime
- **[rusqlite](https://github.com/rusqlite/rusqlite)** — SQLite3 (bundled, no system dependency)
- **[reqwest](https://github.com/seanmonstar/reqwest)** — Inter-service HTTP communication
- **[tower-http](https://github.com/tower-rs/tower-http)** — CORS and request tracing middleware
- **[tokio-util](https://docs.rs/tokio-util)** — Streaming file I/O for range responses

## Design Decisions

**HTTP between services** — Each service runs as an independent process with its own port. This means you can restart, replace, or scale any service without touching the others. The overhead of localhost HTTP is negligible (sub-millisecond).

**SQLite with Mutex** — The catalog uses `Arc<Mutex<rusqlite::Connection>>` with `spawn_blocking` to avoid blocking the async runtime. SQLite is single-writer by design, so a mutex is correct here. WAL mode is enabled for better read concurrency. This can be upgraded to a connection pool or swapped for PostgreSQL without changing the service interface.

**Streaming service coordinates uploads** — When a file is uploaded, the streaming service saves it to disk, then registers the metadata with the catalog service. This keeps file ownership clear and avoids the gateway having to coordinate between two services.

**Single-range only** — Multi-range requests (`bytes=0-100,200-300`) are valid HTTP but rarely used by media players. The streaming service supports single ranges and returns an error for multi-range requests.

## License

MIT
