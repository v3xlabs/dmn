# dmn

a lightweight domain management daemon

## Provider Support

| Provider   | Domains     | DNS         |
| ---------- | ----------- | ----------- |
| Porkbun    | In Progress | In Progress |
| Cloudflare | In Progress | In Progress |


## Installation

Simply copy over the `compose.yml` file to your server and run it.

```bash
docker compose up -d
```

## Configuration

The `compose.yml` file contains a `config.toml` file that you can use to configure the daemon.

```toml
[porkbun]
api_key = "your_api_key"

[cloudflare]
api_key = "your_api_key"
dns = true
```

## Usage

The daemon will automatically keep track of your domains notifying you of new additions, deletions, expiry reminders, and other notifications.

## API Documentation

You can find the OpenAPI Documentation at `http://<host>/docs`
