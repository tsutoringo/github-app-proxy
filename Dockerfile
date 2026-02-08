FROM rust:1.88-alpine AS builder
WORKDIR /app
RUN apk add --no-cache build-base ca-certificates perl
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM alpine:3.20 AS runner
RUN apk add --no-cache ca-certificates \
    && adduser -D -u 10001 app
COPY --from=builder /app/target/release/github-app-proxy /usr/local/bin/github-app-proxy
USER app
EXPOSE 8080
ENTRYPOINT ["github-app-proxy"]
