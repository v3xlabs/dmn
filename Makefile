web:
	cd app/src/web && pnpm "$@"

server:
	cd app/ && cargo run

.PHONY: web server
