# =====================================
# Stage 1: User Setup
# Creates a non-root user and temp directory with correct permissions
# =====================================
FROM alpine:latest as usersetup
RUN addgroup -g 10001 -S dockergrp && \
    adduser -u 10001 -S dockeruser -G dockergrp

# =====================================
# Stage 2: Final Image
# Minimal scratch-based image containing only the essential components
# =====================================
FROM scratch

# Security: Copy user information and switch to non-root user
COPY --from=usersetup /etc/passwd /etc/passwd
USER dockeruser
WORKDIR /app

# Application Configuration
# -----------------------
# RUST_LOG: Logging configuration (default: error,dmn=info)
ENV RUST_LOG="error,dmn=info"
ENV DMN_CONFIG_PATH="/config.toml"
ENV DMN_DATABASE_URL="sqlite://db.sqlite"

# Add temp directory environment variable
ENV TMPDIR=/tmp

# Binary Installation
# -----------------
# BINARY_PATH: Path to the pre-compiled binary from GitHub Actions
ARG BINARY_PATH
COPY ${BINARY_PATH} /app/dmn
# COPY www /app/www

# Network Configuration
# -------------------
EXPOSE 3000

# Application Startup
# -----------------
# Using exec form of CMD as there is no shell in scratch image
CMD ["/app/dmn", "server"]
