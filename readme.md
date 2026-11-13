# tpad

A minimal notepad server. Visit it in your browser, get a pad, start typing. Saves automatically.

## Install

```sh
git clone https://github.com/toonalfrink/tpad
cd tpad && ./tpad install
```

Then open `http://localhost:3000`.

## Install (Docker)

```sh
git clone https://github.com/toonalfrink/tpad
cd tpad && docker compose up -d --build
```

Then open `http://localhost:3000`. Pads are persisted in the `tpad-data` volume.

To stop or remove it:

```sh
docker compose down          # stop
docker compose down -v       # stop and delete all pads
```

### Local settings (override)

`docker compose up` automatically merges `docker-compose.yml` with a
`docker-compose.override.yml` if present, so you can keep personal settings
out of the repo. A ready-to-edit template is included:

```sh
cp docker-compose.override.yml.example docker-compose.override.yml
```

The example shows the two most common tweaks — publishing on a different host
port and storing pads in a local folder you can browse/edit directly:

```yaml
services:
  tpad:
    ports: !override
      - "8080:3000"
    volumes: !override
      - ./data:/data
```

Then run `docker compose up -d --build` as usual.

> Note: list values like `ports` and `volumes` are merged (unioned) by default.
> Use the `!override` tag (Docker Compose v2.24+) to replace them entirely, as
> shown above. Plain values like `environment` are replaced automatically.

## Commands

| Command | Description |
|---|---|
| `tpad install` | Build, install, and start the systemd service |
| `tpad run` | Compile and run locally (no systemd) |
| `tpad uninstall` | Stop the service and delete all data |

## Configuration

| Variable | Default | Description |
|---|---|---|
| `PORT` | `3000` | Port to listen on (auto-increments if taken) |
| `TPAD_DATA_DIR` | `~/.local/share/tpad` | Where pads are stored |

## Requirements

- [Docker](https://www.docker.com/) — for the Docker install above

Or, for a native install:

- [Rust](https://rustup.rs) — `rustc` in PATH
- [Nushell](https://www.nushell.sh) — `nu` in PATH
- `systemd`
