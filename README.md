# dmn

[![Docker Image 16MB](https://img.shields.io/badge/Docker%20Image-%3C16MB-brightgreen)](https://ghcr.io/v3xlabs/dmn) ![Porkbun Support](https://img.shields.io/badge/Porkbun-Supported-EF7878?logo=porkbun) ![Cloudflare Support](https://img.shields.io/badge/Cloudflare-Supported-F38020?logo=cloudflare)

a lightweight domain management daemon

## Provider Support

| Provider   | Domains                 | DNS                                                              |
| ---------- | ----------------------- | ---------------------------------------------------------------- |
| Porkbun    | ✅ Implemented          | ❌ Not implemented yet                                           |
| Cloudflare | ✅ Using Global API Key | ✅ Using Token API Key (DNS::Read, Zone::Read) or Global API Key |
| ...        | ...                     | ...                                                              |

## Installation

Simply copy over the `compose.yml` file to your server and run it.

```yml
# DMN - A lightweight domain management service
name: dmn
services:
  dmn:
    image: ghcr.io/v3xlabs/dmn:edge
    environment:
      DATABASE_URL: sqlite://data/sqlite.db
      JWT_SECRET: abc123
      PORKBUN_API_KEY: abc123
      CLOUDFLARE_API_KEY: abc123
      RUST_LOG: info
    ports:
      - "3000:3000"

```

And then when you're ready to start the daemon, run the following command:

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

### Cloudflare Token

When creating a cloudflare token visit [the dashboard](https://dash.cloudflare.com/profile/api-tokens) and create a new token with the following permissions:

- Zone: Zone Read
- Zone: DNS Read

For most purposes you will want to select `Include All Zones`, however if you wish to limit the scope of the token you are more then welcome to.

If you wish you use `domains` however you will need to use your `Global API Key` which you can find in the dashboard.
This is due to cloudflare lacking a read-only domain API scope.

### Porkbun API Key

To get a porkbun api key visit [the dashboard](https://porkbun.com/account/api).

## Usage

The daemon will automatically keep track of your domains notifying you of new additions, deletions, expiry reminders, and other notifications.

## API Documentation

You can find the OpenAPI Documentation at `http://<host>/docs`
