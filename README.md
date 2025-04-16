# dmn

a lightweight domain management daemon

## Provider Support

| Provider   | Domains     | DNS         |
| ---------- | ----------- | ----------- |
| Porkbun    | In Progress | In Progress |
| Cloudflare | In Progress | In Progress |

## Installation

Simply copy over the `compose.yml` file to your server and run it.

```yml
name: dmn
services:
  dmn:
    image: ghcr.io/v3xlabs/dmn:edge
    environment:
      DATABASE_URL: postgres://postgres:postgres@postgres:5432/dmn
      JWT_SECRET: abc123
      PORKBUN_API_KEY: abc123
      CLOUDFLARE_API_KEY: abc123
    ports:
      - "3000:3000"
    depends_on:
      - postgres

  postgres:
    image: postgres:17
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: postgres # It is recommended to change this to a more secure password
      POSTGRES_DB: dmn
    ports:
      - "5432:5432"
    volumes:
      - pg-dmn-data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres -d postgres"]
      interval: 5s
      timeout: 5s
      retries: 5

volumes:
  pg-dmn-data:
   driver: local
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

## Usage

The daemon will automatically keep track of your domains notifying you of new additions, deletions, expiry reminders, and other notifications.

## API Documentation

You can find the OpenAPI Documentation at `http://<host>/docs`
