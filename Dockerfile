FROM ghcr.io/getzola/zola:v0.21.0 as zola

COPY . /target
WORKDIR /target
RUN ["zola", "build"]

FROM ghcr.io/static-web-server/static-web-server:2

WORKDIR /public
COPY --from=zola /target/public /public
EXPOSE 8180
CMD ["static-web-server", "/public", "--port", "8180"]

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8180/ || exit 1
