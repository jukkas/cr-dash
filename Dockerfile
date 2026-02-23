FROM ghcr.io/blackdex/rust-musl:x86_64-musl AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl


FROM alpine AS runtime
WORKDIR /app
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/cr-dash .
COPY ./static /app/static
USER 1000
EXPOSE 3000
ENTRYPOINT ["/app/cr-dash"]