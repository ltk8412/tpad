# build stage: compile the single-file rust binary
FROM rust:1.82-slim AS builder

WORKDIR /build
COPY src/ ./src/

RUN rustc -O src/main.rs -o tpad

# runtime stage: minimal image
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/tpad /usr/local/bin/tpad

ENV PORT=3000 \
    TPAD_DATA_DIR=/data

EXPOSE 3000

CMD ["tpad"]
