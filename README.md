# stream-media

A microservices-based media streaming platform built in Rust. Supports adaptive bitrate streaming via HLS and DASH, direct file streaming with HTTP Range requests, Samba/CIFS network share integration, and user management with password authentication.

## Architecture

```
Client
  │
  ▼
Gateway (:3000)
  ├── /api/media/*     → Catalog Service (:3001)
  ├── /api/sources/*   → Catalog Service (:3001)
  ├── /api/users/*     → User Service (:3003)
  ├── /stream/*        → Streaming Service (:3002)
  ├── /upload          → Streaming Service (:3002)
  └── /transcode/*     → Streaming Service (:3002)
```

| Crate | Port | Purpose |
|-------|------|---------|
| `gateway` | 3000 | Reverse proxy, CORS, single entry point |
| `catalog-service` | 3001 | Media metadata, SMB source management (SQLite) |
| `streaming-service` | 3002 | File upload, HLS/DASH transcoding, adaptive streaming |
| `user-service` | 3003 | User management with argon2 password hashing (SQLite) |
| `common` | — | Shared types, error handling, configuration |

## Streaming Methods

Configured at setup time via `STREAMING_METHOD`:

| Method | Description |
|--------|-------------|
| **HLS** | Videos transcoded to 360p/720p/1080p. Serves `.m3u8` playlists + `.ts` segments. Widest player support. |
| **DASH** | Videos transcoded to 360p/720p/1080p. Serves `.mpd` manifest + `.m4s` segments. Open standard. |
| **HTTP Range** | Serves original files as-is with byte-range seeking. No transcoding overhead. |

HLS and DASH both produce three adaptive quality variants. Players automatically switch quality based on network conditions.

## Getting Started

### 1. Initial Setup

Run the interactive setup script to configure admin credentials and streaming method:

```bash
./setup.sh
```

This generates a `.env` file with:
- Admin username, email, and password
- Streaming method selection (HLS, DASH, or HTTP Range)

### 2. Start Services

**With Docker/Podman Compose (recommended):**

```bash
docker compose up --build -d
```

**Without containers:**

Prerequisites: Rust (edition 2024), ffmpeg (for HLS/DASH transcoding)

```bash
cargo build
source .env

# Start each service (separate terminals)
cargo run -p catalog-service
cargo run -p streaming-service
cargo run -p user-service
cargo run -p gateway
```

### 3. Try It

```bash
# Upload a video (auto-transcodes if HLS or DASH mode)
curl -F "title=My Video" -F "media_type=video" -F "format=mp4" \
     -F "file=@video.mp4" http://localhost:3000/upload

# Check transcode progress
curl http://localhost:3000/transcode/{id}/status

# Stream via HLS (adaptive bitrate)
curl http://localhost:3000/stream/{id}/hls/master.m3u8

# Stream via DASH (adaptive bitrate)
curl http://localhost:3000/stream/{id}/dash/manifest.mpd

# Direct stream with range seeking
curl -H "Range: bytes=0-65535" http://localhost:3000/stream/{id}

# Manage users
curl -X POST -H "Content-Type: application/json" \
     -d '{"username":"alice","email":"alice@example.com","password":"secret"}' \
     http://localhost:3000/api/users

# Register an SMB share, mount it, add media from it
curl -X POST -H "Content-Type: application/json" \
     -d '{"name":"NAS","server":"nas.local","share_name":"media","username":"reader","password":"pass"}' \
     http://localhost:3000/api/sources/smb

curl -X POST http://localhost:3000/api/sources/smb/{source_id}/mount

curl -X POST -H "Content-Type: application/json" \
     -d '{"title":"Movie","media_type":"video","format":"mkv","source_id":"...","path":"movies/film.mkv"}' \
     http://localhost:3000/api/media/register-smb
```

## API

All endpoints are accessed through the gateway on port 3000.

### Users

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/users` | Create user (optional `password`) |
| `GET` | `/api/users` | List users (`search`, `limit`, `offset`) |
| `GET` | `/api/users/{id}` | Get user |
| `PUT` | `/api/users/{id}` | Update user |
| `DELETE` | `/api/users/{id}` | Delete user |

### Media

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/media` | Create metadata record |
| `GET` | `/api/media` | List media (`search`, `media_type`, `limit`, `offset`) |
| `GET` | `/api/media/{id}` | Get media item |
| `PUT` | `/api/media/{id}` | Update metadata |
| `DELETE` | `/api/media/{id}` | Delete media item |
| `POST` | `/api/media/register-smb` | Register media from mounted SMB source |

### SMB Sources

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/sources/smb` | Register an SMB share |
| `GET` | `/api/sources/smb` | List all sources |
| `GET` | `/api/sources/smb/{id}` | Get source details |
| `PUT` | `/api/sources/smb/{id}` | Update source |
| `DELETE` | `/api/sources/smb/{id}` | Delete source (blocked if media references it) |
| `POST` | `/api/sources/smb/{id}/mount` | Mount the share via CIFS |
| `POST` | `/api/sources/smb/{id}/unmount` | Unmount the share |

### Upload & Direct Streaming

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/upload` | Upload media file (multipart/form-data) |
| `GET` | `/stream/{id}` | Stream original file (supports `Range` header) |

### HLS Streaming

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/stream/{id}/hls/master.m3u8` | Master playlist (lists quality variants) |
| `GET` | `/stream/{id}/hls/{variant}/playlist.m3u8` | Variant playlist (360p/720p/1080p) |
| `GET` | `/stream/{id}/hls/{variant}/{segment}` | MPEG-TS segment |

### DASH Streaming

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/stream/{id}/dash/manifest.mpd` | MPD manifest |
| `GET` | `/stream/{id}/dash/{repr}/{file}` | Init/chunk segments (.m4s/.mp4) |

### Transcode Control

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/transcode/{id}` | Trigger transcoding (uses configured method) |
| `GET` | `/transcode/{id}/status` | Check status + available variants |

## Configuration

Generated by `setup.sh` and read from `.env`:

| Variable | Default | Description |
|----------|---------|-------------|
| `ADMIN_USERNAME` | — | Admin user seeded on first boot |
| `ADMIN_EMAIL` | — | Admin email address |
| `ADMIN_PASSWORD` | — | Admin password (hashed with argon2) |
| `STREAMING_METHOD` | `hls` | `hls`, `dash`, or `range` |
| `TRANSCODE_MAX_JOBS` | `2` | Max concurrent ffmpeg transcode processes |
| `GATEWAY_PORT` | `3000` | Gateway listen port |
| `CATALOG_PORT` | `3001` | Catalog service listen port |
| `STREAMING_PORT` | `3002` | Streaming service listen port |
| `USER_PORT` | `3003` | User service listen port |
| `CATALOG_URL` | `http://127.0.0.1:3001` | Catalog service URL |
| `STREAMING_URL` | `http://127.0.0.1:3002` | Streaming service URL |
| `USER_URL` | `http://127.0.0.1:3003` | User service URL |
| `DATABASE_PATH` | `./catalog.db` | Catalog SQLite database |
| `USER_DATABASE_PATH` | `./users.db` | User SQLite database |
| `MEDIA_STORE_PATH` | `./media-store` | Uploaded files and transcoded output |
| `SMB_MOUNT_BASE` | `/mnt/smb` | Base path for SMB share mounts |

Set `RUST_LOG` to control log verbosity (e.g. `RUST_LOG=debug`).

## Project Structure

```
stream-media/
├── setup.sh                          # Interactive setup script
├── Cargo.toml                        # Workspace definition
├── Containerfile                     # Multi-stage build (4 service targets)
├── compose.yaml                      # Container orchestration (reads .env)
├── common/
│   └── src/
│       ├── lib.rs                    # Module exports
│       ├── models.rs                 # MediaItem, User, SmbSource, TranscodeStatus, etc.
│       ├── error.rs                  # AppError with axum IntoResponse
│       └── config.rs                 # ServiceConfig from environment
├── catalog-service/
│   └── src/
│       ├── main.rs
│       ├── db.rs                     # SQLite: media CRUD, SMB sources, transcode status
│       ├── handlers.rs
│       └── routes.rs
├── streaming-service/
│   └── src/
│       ├── main.rs
│       ├── handlers.rs               # Upload, direct stream, HLS/DASH serving, transcode control
│       ├── transcode.rs              # ffmpeg HLS/DASH transcoding pipeline
│       ├── range.rs                  # HTTP Range parsing and 206 responses
│       └── routes.rs
├── user-service/
│   └── src/
│       ├── main.rs                   # Entry point + admin seeding
│       ├── db.rs                     # SQLite: user CRUD with argon2 password hashing
│       ├── handlers.rs
│       └── routes.rs
└── gateway/
    └── src/
        ├── main.rs
        ├── proxy.rs                  # Reverse proxy logic
        └── routes.rs                 # Route mapping and CORS
```

## Tech Stack

- **[axum](https://github.com/tokio-rs/axum)** — HTTP framework
- **[tokio](https://tokio.rs)** — Async runtime
- **[rusqlite](https://github.com/rusqlite/rusqlite)** — SQLite3 (bundled, no system dependency)
- **[argon2](https://github.com/RustCrypto/password-hashes)** — Password hashing
- **[reqwest](https://github.com/seanmonstar/reqwest)** — Inter-service HTTP communication
- **[tower-http](https://github.com/tower-rs/tower-http)** — CORS and request tracing
- **[tokio-util](https://docs.rs/tokio-util)** — Streaming file I/O for range responses
- **ffmpeg** — HLS/DASH transcoding (runtime dependency, not linked)

## Design Decisions

**HTTP between services** — Each service is an independent process. You can restart, replace, or scale any service without touching the others. Localhost HTTP overhead is sub-millisecond.

**SQLite with Mutex** — `Arc<Mutex<rusqlite::Connection>>` with `spawn_blocking`. SQLite is single-writer by design, so a mutex is correct. WAL mode for read concurrency. Swappable for PostgreSQL without changing service interfaces.

**Streaming service coordinates uploads** — On upload, the streaming service saves the file to disk, registers metadata with the catalog, then spawns a transcode job if needed. File ownership stays clear.

**Sequential variant transcoding** — Each quality variant (360p/720p/1080p) is transcoded one at a time per video. A semaphore limits concurrent jobs across videos. This prevents CPU exhaustion on self-hosted hardware.

**SMB source-first model** — Network shares are registered and mounted before media can be added from them (Kodi-style). Credentials stay on the source record and are never serialized in API responses. Mount operations use temporary credential files to avoid password exposure in process lists.

## License

MIT
