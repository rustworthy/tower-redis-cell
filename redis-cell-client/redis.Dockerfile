################################ BUILDER ######################################
FROM rust:1.89-bookworm AS builder

WORKDIR /redis-cell

RUN git clone -b valkey https://github.com/rustworthy/redis-cell.git .
RUN cargo build --release

################################ RUNTIME ######################################
FROM redis:8.2.2-bookworm AS runtime

COPY --from=builder /redis-cell/target/release/libredis_cell.so /usr/local/lib/redis/modules/libredis_cell.so

USER redis

CMD ["redis-server"]
