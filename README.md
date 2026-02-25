# CR-dash

A small Rust web dashboard for exploring a Docker/OCI registry (Distribution Registry) Repositories + Tags with GitHub OAuth access control.
Login access is limited to single configurable Github Organization.

Key points
- Built with `axum` for the HTTP server and routing.
- Templates rendered with `askama` (see `templates/`).
- Uses GitHub OAuth to restrict access to members of a specified organization.
- Fetches repositories and tags from a container registry (CR) using basic auth and caches results for a short period.
- Session state stored in secure cookies (cookie sessions).

Features
- List repositories from a registry's `_catalog` endpoint.
- View tags for a selected repository.
- Login via GitHub and restrict access to members of a configured org.
- Simple request logging and a static assets fallback for `static/` files.

Quickstart (development)

1. Copy or set the required environment variables. At minimum the app expects:

- `CR_URL` — base URL of your registry (e.g. `https://registry.example.com`).
- `CR_USERNAME` / `CR_PASSWORD` — credentials for registry basic auth.
- `GITHUB_CLIENT_ID` / `GITHUB_CLIENT_SECRET` — GitHub OAuth app credentials.
- `GITHUB_ORG` — GitHub organization login used to gate access.
- `APP_PORT` — port the app will bind to (defaults to `3000`).
- `SESSION_KEY` — base64-encoded 64-byte key for cookie session encryption (optional; a default is used in development).

Example `.env` (development only):

```
CR_URL=https://registry.example.com
CR_USERNAME=svc-account
CR_PASSWORD=supersecret
GITHUB_CLIENT_ID=xxxxx
GITHUB_CLIENT_SECRET=yyyyy
GITHUB_ORG=my-org
APP_PORT=3000
SESSION_KEY=NNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNNN==
```

2. Run locally:

```
cargo run --release
```

3. Open your browser at `http://localhost:3000` (or the host/port you configured).

Docker

There is a `Dockerfile` in the repo. Build and run with:

```
docker build -t cr-dash .
docker run -e CR_URL=... -e CR_USERNAME=... -e CR_PASSWORD=... -e GITHUB_CLIENT_ID=... -e GITHUB_CLIENT_SECRET=... -e GITHUB_ORG=... -p 3000:3000 cr-dash
```

Configuration and environment

- `APP_PORT` — TCP port to bind (default `3000`).
- `SESSION_KEY` — Base64 encoded key used to encrypt cookie sessions. If unset, a development key is printed (not secure for production).
- `CR_URL`, `CR_USERNAME`, `CR_PASSWORD` — used to call registry endpoints `/v2/_catalog` and `/v2/{name}/tags/list`.
- `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, `GITHUB_ORG` — required for login and org membership checks.

Development notes

- Templates are located in the `templates/` directory. The visible pages are built with `askama` templates: `public.html`, `repositories.html`, `tags.html`, and `base.html`.
- Static assets (CSS, JS) are served from the `static/` directory.
- The app caches the list of repositories for 5 minutes to reduce upstream load (see `src/containers.rs`).
- Authentication flow lives in `src/github.rs` and uses GitHub OAuth to fetch the authenticated user's orgs.

Security

- Do not use the default `SESSION_KEY` in production. Provide a securely generated 64-byte key encoded in base64.
- Run the server behind TLS/HTTPS or a reverse proxy that terminates TLS.

Where to look in the code

- Server and routing: `src/main.rs`
- Registry repositories / tags handling: `src/containers.rs`
- GitHub OAuth and callback handling: `src/github.rs`
- Templates: `templates/`

