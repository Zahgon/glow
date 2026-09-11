FROM rust:1.85-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tests ./tests
RUN cargo build --release --locked --bin glow

FROM gcr.io/distroless/static
COPY --from=build /src/target/release/glow /usr/local/bin/glow
ENTRYPOINT [ "/usr/local/bin/glow" ]
