# Links HTTP Client

A browser-based HTTP workbench for the Links account service, built with Vite,
Matrix, and Prism.

## Run locally

```sh
npm install
npm run dev
```

Open `http://localhost:5174`. Requests using the default `/links-api` base URL
are proxied to `http://127.0.0.1:8080`, avoiding browser CORS restrictions.
Override the target when starting Vite:

```sh
LINKS_HTTP_TARGET=https://links.example.test npm run dev
```

You can also enter an absolute base URL in the app. Direct cross-origin URLs
must allow the browser origin through their CORS policy.

## Security

Bearer tokens and admin keys remain in memory and are never written to request
history or browser storage. History stores only the method, path, authentication
mode, response status, duration, and timestamp. Clear it from the History pane.

The cURL action includes the active credential because it reproduces the current
request. Treat the copied command as sensitive.

## Production build

```sh
npm run build
npm run preview
```

A deployed static build needs a same-origin `/links-api` reverse proxy or a Links
endpoint that explicitly permits the deployed origin through CORS.
