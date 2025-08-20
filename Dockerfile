FROM ghcr.io/getzola/zola:v0.21.0 AS zola

COPY . /target
WORKDIR /target
RUN ["zola", "build"]

FROM ghcr.io/static-web-server/static-web-server:2

WORKDIR /
COPY --from=zola /target/public /public
