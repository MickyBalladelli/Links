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

This app is an interactive client UI preview. It persists conversations, contacts, group membership, image messages, and file messages locally. Images and files can be selected, pasted, or dropped into the composer; attachment bytes are kept in IndexedDB while conversation metadata stays in local storage. A bearer token enables username resolution and profile updates through the authenticated account API.

The app does not claim to send encrypted network messages yet: the shared browser WASM messaging core, durable encrypted stores, pairing/session restoration, pre-key setup, group MLS operations, attachment upload/download adapters, and `WebTextMessaging` transport still need to be composed into this host. The connection banner and profile status keep that boundary visible in the interface.
