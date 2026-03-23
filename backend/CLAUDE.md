# b01-backend

Comment system backend for betweenzeroand.one. Axum + SQLite + HTMX.

## Build & Run

```
cargo run                                    # dev
CORS_ORIGIN=http://127.0.0.1:1111 cargo run   # dev with Zola frontend
```

## Env Vars

- `DATABASE_URL` — SQLite path (default: `sqlite:comments.db`)
- `USERINFO_URL` — OIDC userinfo endpoint (default: Rauthy at b01-idm.com)
- `CORS_ORIGIN` — allowed origin (default: `https://www.betweenzeroand.one`)
- `LISTEN_ADDR` — bind address (default: `0.0.0.0:3001`)

## Architecture

- Auth: validates Bearer tokens against Rauthy's OIDC userinfo endpoint via `ureq` (blocking, wrapped in `spawn_blocking`)
- Endpoints return HTML fragments for HTMX, not JSON
- Frontend lives in `../b01-zola`
