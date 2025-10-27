FROM valkey/valkey:9.0.0

ARG CELL_VERSION=0.4.0

RUN apt-get -y update && apt-get -y upgrade && \
  apt-get -y install wget ca-certificates --no-install-recommends && \
  rm -rf /var/lib/apt/lists/*

RUN mkdir /tmp/redis-cell && \
  wget -qO /tmp/redis-cell/redis-cell.tar.gz "https://github.com/brandur/redis-cell/releases/download/v${CELL_VERSION}/redis-cell-v${CELL_VERSION}-x86_64-unknown-linux-gnu.tar.gz" && \
  tar zxf /tmp/redis-cell/redis-cell.tar.gz --directory=/tmp/redis-cell && \
  mkdir -p /usr/local/lib/valkey/modules && \
  mv /tmp/redis-cell/libredis_cell.so "/usr/local/lib/valkey/modules/libredis_cell-v${CELL_VERSION}.so" && \
  chown -R valkey:valkey /usr/local/lib/valkey && \
  rm -r /tmp/redis-cell

USER valkey

# shell indirection required to be able to refer to the `REDIS_CELL_VERSION` variable,
# see: https://stackoverflow.com/a/40454758
CMD ["sh", "-c", "valkey-server", "--loadmodule", "/usr/local/lib/valkey/libredis_cell-v${CELL_VERSION}.so"]
