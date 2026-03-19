FROM rust:1.77 as builder
WORKDIR /app
COPY . .
RUN cargo build --release -p kw-api

FROM debian:bookworm-slim
RUN useradd -m appuser
WORKDIR /app
COPY --from=builder /app/target/release/kw-api /usr/local/bin/kw-api
USER appuser
EXPOSE 3000
CMD ["kw-api"]
