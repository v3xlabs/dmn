# dmn

[![Docker Image 16MB](https://img.shields.io/badge/Docker%20Image-%3C16MB-brightgreen)](https://ghcr.io/v3xlabs/dmn) ![Porkbun Support](https://img.shields.io/badge/Porkbun-Supported-EF7878?logo=porkbun) ![Cloudflare Support](https://img.shields.io/badge/Cloudflare-Supported-F38020?logo=cloudflare) ![Docs using OpenAPI](https://img.shields.io/badge/Docs-OpenAPI-brightgreen?logo=swagger)

A lightweight domain management daemon

## Installation

### Binary & CLI

You can download the binary from the [releases page](https://github.com/v3xlabs/dmn/releases).
The binary can be used as a cli tool or as a service when invoked as `dmn server`.

### Docker

Simply copy over the `compose.yml` file to your server and run it.

```yml
# DMN - A lightweight domain management service
name: dmn
services:
    dmn:
        image: ghcr.io/v3xlabs/dmn:0.0.8
        environment:
            DMN_API_SECRET: abcdefg123456789
            PORKBUN_API_KEY: abcdefg123456789
            CLOUDFLARE_API_KEY: abcdefg123456789
            CLOUDFLARE_EMAIL: hello@example.com
        volumes:
            - ./sqlite.db:/data/sqlite.db
        ports:
            - "3000:3000"
```

## Usage

The daemon will automatically keep track of your domains notifying you of new additions, deletions, expiry reminders, and other notifications.

-   `dmn ls` - List all domains
-   `dmn porkbun`
    -   `dmn porkbun index` - Index your porkbun domains
-   `dmn cloudflare`
    -   `dmn cloudflare index` - Index your cloudflare domains
-   `dmn whois`
    -   `dmn whois example.com` - Get the whois information example.com
    - `dmn whois --json example.com`
-   `dmn server` - Start the daemon in server mode

## Provider Support

| Provider   | Domains                 | DNS                                                              |
| ---------- | ----------------------- | ---------------------------------------------------------------- |
| Porkbun    | ✅ Implemented          | ❌ Not implemented yet                                           |
| Cloudflare | ✅ Using Global API Key | ✅ Using Token API Key (DNS::Read, Zone::Read) or Global API Key |
| ...        | ...                     | ...                                                              |

## Configuration

You can configure the daemon by providing any of the configuration variables as either environment variables or by providing a `config.toml` file. You can find an [example config file](./app/config.toml) in the root of the repository.

| Variable   | Required                 | Description                                             |
| ---------- | ------------------------ | ------------------------------------------------------- |
| API Secret | Required for server mode | random value                                            |
| Calendar   | Optional                 | calendar generation (`.ics` format)                     |
| RSS        | Optional                 | expiry & registration rss generation (`rss.xml` format) |
| Porkbun    | Optional                 | domains & dns                                           |
| Cloudflare | Optional                 | domains & dns                                           |

### Calendar

The calendar feature allows you to generate a calendar of when your domains are expiring.
You can enable the calendar and configure it in the `config.toml` file.

```toml
[calendar]
enabled = true

```

### RSS

The rss feature allows you to generate an rss feed of your newly registered or expiring domains.
You can enable the rss feature and configure it in the `config.toml` file.

```toml
[rss]
enabled = true
warn_before = "30 days"
```

The feeds will be available at `http://<host>:3000/api/expiration.xml` and `http://<host>:3000/api/registration.xml`.

### Providers

#### Cloudflare Token

When creating a cloudflare token visit [the dashboard](https://dash.cloudflare.com/profile/api-tokens) and create a new token with the following permissions:

-   Zone: Zone Read
-   Zone: DNS Read

For most purposes you will want to select `Include All Zones`, however if you wish to limit the scope of the token you are more then welcome to.

If you wish you use `domains` however you will need to use your `Global API Key` which you can find in the dashboard.
This is due to cloudflare lacking a read-only domain API scope.

#### Porkbun API Key

To get a porkbun api key visit [the dashboard](https://porkbun.com/account/api).

## API Documentation

You can find the OpenAPI Documentation at `http://<host>:3000/docs`
