# docker build -f deploy/Dockerfile -t p2m-node .
# docker run -d --name p2m -p 4001:4001/tcp -p 4001:4001/udp -v p2m:/data p2m-node
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p p2m-node

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/p2m-node /usr/local/bin/p2m-node
VOLUME /data
EXPOSE 4001/tcp 4001/udp
ENV RUST_LOG=info
ENTRYPOINT ["p2m-node", "--headless", "--data", "/data", "--port", "4001"]
CMD ["--relay-server"]
