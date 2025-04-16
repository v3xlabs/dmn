# dmn

[![Docker Image 16MB](https://img.shields.io/badge/Docker%20Image-%3C16MB-brightgreen)](https://ghcr.io/v3xlabs/dmn) ![Porkbun Support](https://img.shields.io/badge/Porkbun-Supported-EF7878?logo=porkbun) ![Cloudflare Support](https://img.shields.io/badge/Cloudflare-Supported-F38020?logo=cloudflare)

a lightweight domain management daemon

## Installation

### Binary

You can download the binary from the [releases page](https://github.com/v3xlabs/dmn/releases).
The binary can be used as a cli tool or as a service when invoked as `dmn server`.

### Docker

Simply copy over the `compose.yml` file to your server and run it.

```yml
# DMN - A lightweight domain management service
name: dmn
services:
  dmn:
    image: ghcr.io/v3xlabs/dmn:0.0.1
    environment:
      DATABASE_URL: sqlite://data/sqlite.db
      JWT_SECRET: abc123
      PORKBUN_API_KEY: abc123
      CLOUDFLARE_API_KEY: abc123
      RUST_LOG: info
    volumes:
      - ./sqlite.db:/data/sqlite.db
    ports:
      - "3000:3000"
```

## Configuration

### Cloudflare Token

When creating a cloudflare token visit [the dashboard](https://dash.cloudflare.com/profile/api-tokens) and create a new token with the following permissions:

- Zone: Zone Read
- Zone: DNS Read

For most purposes you will want to select `Include All Zones`, however if you wish to limit the scope of the token you are more then welcome to.

If you wish you use `domains` however you will need to use your `Global API Key` which you can find in the dashboard.
This is due to cloudflare lacking a read-only domain API scope.

### Porkbun API Key

To get a porkbun api key visit [the dashboard](https://porkbun.com/account/api).

## Provider Support

| Provider   | Domains                 | DNS                                                              |
| ---------- | ----------------------- | ---------------------------------------------------------------- |
| Porkbun    | ✅ Implemented          | ❌ Not implemented yet                                           |
| Cloudflare | ✅ Using Global API Key | ✅ Using Token API Key (DNS::Read, Zone::Read) or Global API Key |
| ...        | ...                     | ...                                                              |

## Usage

The daemon will automatically keep track of your domains notifying you of new additions, deletions, expiry reminders, and other notifications.

- `dmn porkbun index` - Index your porkbun domains
- `dmn cloudflare index` - Index your cloudflare domains
- `dmn domains ls` - List all domains
- `dmn whois example.com` - Get the whois information for a domain
- `dmn server` - Start the daemon in server mode

## API Documentation

You can find the OpenAPI Documentation at `http://<host>/docs`
