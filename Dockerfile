FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src src
COPY static static
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12
COPY --from=build /src/target/release/photo-post-prep /app
ENV PORT=8080
EXPOSE 8080
ENTRYPOINT ["/app"]
