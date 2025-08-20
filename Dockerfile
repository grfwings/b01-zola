FROM ghcr.io/getzola/zola:v0.17.1 AS builder

COPY . /project
WORKDIR /project

RUN zola build

FROM ghcr.io/static-web-server/static-web-server:2

WORKDIR /public

COPY --from=builder /project/public /public

EXPOSE 8080

CMD ["static-web-server", "/public", "--port", "8080"]

