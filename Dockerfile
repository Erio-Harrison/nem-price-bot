FROM rust:1.82-slim AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/nem-price-bot /usr/local/bin/
VOLUME /data
ENV DATABASE_URL=/data/nem_price.db
CMD ["nem-price-bot"]
