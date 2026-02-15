# rpglot-web

Web UI and REST API server for rpglot.

## Build

The frontend (React) is pre-built and committed to `frontend/dist/`.
No Node.js is required for building the binary:

```bash
cargo build -p rpglot-web --release
```

`rust-embed` embeds `frontend/dist/` into the binary at compile time.

## Updating the frontend

When you change frontend source code (`frontend/src/`), rebuild the dist:

```bash
cd crates/rpglot-web/frontend
npm install        # first time or after package.json changes
npm run build      # tsc + vite build -> dist/
```

Then commit the updated `frontend/dist/` and rebuild the Rust binary.

### Frontend dev mode

For fast iteration without rebuilding the binary:

```bash
# Terminal 1: start the backend
cargo run -p rpglot-web -- --pg "host=localhost dbname=postgres"

# Terminal 2: start vite dev server with API proxy
cd crates/rpglot-web/frontend
npm run dev
```

Vite proxies `/api` requests to `http://127.0.0.1:8080` (configured in `vite.config.ts`).

## Run

```bash
# Live mode
rpglot-web --pg "host=localhost dbname=postgres"

# History mode
rpglot-web --history /path/to/data

# With auth
rpglot-web --pg "..." --auth-user admin --auth-password secret
```
