# docker build -f deploy/Dockerfile -t lacre-node .
# docker run -d --name lacre -p 4001:4001/tcp -p 4001:4001/udp -v lacre:/data lacre-node
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p lacre-node

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/lacre-node /usr/local/bin/lacre-node
VOLUME /data
EXPOSE 4001/tcp 4001/udp
ENV RUST_LOG=info
ENTRYPOINT ["lacre-node", "--headless", "--data", "/data", "--port", "4001"]
CMD ["--relay-server"]
