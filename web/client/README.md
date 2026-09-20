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

This app is an interactive client UI preview. It persists preview conversations and contacts locally and can resolve usernames through the authenticated directory API when a bearer token is supplied. It does not claim to send encrypted network messages yet: the shared browser WASM messaging core, durable stores, pairing/session restoration, pre-key setup, and `WebTextMessaging` transport still need to be composed into this host.

The connection banner and profile status keep that boundary visible in the interface.
