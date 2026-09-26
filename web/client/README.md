# Links Web Client UI

A browser client shell for Links, built with the same Vite, Matrix, and Prism packages as the HTTP workbench.

## Run locally

```sh
npm install
npm run dev
```

Open `http://localhost:5175`.

The default `/links-api` path proxies to `http://127.0.0.1:8080`. Override it with:

```sh
LINKS_AUTH_TARGET=https://links.example.test npm run dev
```

## Current integration boundary

This app keeps the local conversation shell and attachment cache in the browser, then starts the shared Rust/WASM messaging core after account authentication. The core publishes browser pre-keys, primes recipient devices, creates encrypted MLS/Sealed Sender frames, and reconnects through the binary `/v1/connect` WebSocket. The browser WASM bundle is included in `public/web-wasm`; refresh it with `npm run build:wasm` from `web` after changing the Rust core.

The browser core keeps MLS and sealed-sender state in WASM. The host owns browser metadata, WebSocket reconnect, directory/pre-key HTTP adapters, and rendering. Group/media transport and a reviewed durable WASM state provider remain separate follow-up work; missing core assets never fall back silently to a fake connected state.
