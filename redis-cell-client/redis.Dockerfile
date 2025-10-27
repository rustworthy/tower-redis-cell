FROM redis:8.2.2

ARG CELL_VERSION=0.4.0

RUN apt-get -y update && apt-get -y upgrade && \
  apt-get -y install wget ca-certificates --no-install-recommends && \
  rm -rf /var/lib/apt/lists/*


RUN mkdir /tmp/redis-cell && \
  wget -qO /tmp/redis-cell/redis-cell.tar.gz "https://github.com/brandur/redis-cell/releases/download/v${CELL_VERSION}/redis-cell-v${CELL_VERSION}-x86_64-unknown-linux-gnu.tar.gz" && \
  tar zxf /tmp/redis-cell/redis-cell.tar.gz --directory=/tmp/redis-cell && \
  # redis will auto-load modules located in "/usr/local/lib/redis/modules"
  mv /tmp/redis-cell/libredis_cell.so "/usr/local/lib/redis/modules/libredis_cell-v${CELL_VERSION}.so" && \
  rm -r /tmp/redis-cell

USER redis

CMD ["redis-server"]


