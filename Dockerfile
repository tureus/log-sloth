# -*- mode: dockerfile -*-
#
# An example Dockerfile showing how to build a Rust executable using this
# image, and deploy it with a tiny Alpine Linux container.

# Our first FROM statement declares the build environment.
FROM ekidd/rust-musl-builder AS builder

# Add our source code.
ADD . ./

# Fix permissions on source code.
RUN sudo chown -R rust:rust /home/rust

# Build our application.
RUN cargo build --release

# Now, we need to build our _real_ Docker container, copying in `using-diesel`.
FROM alpine:latest
ADD docker/apk-repositories /etc/apk/repositories
RUN apk --no-cache add ca-certificates htop sysstat procps
COPY --from=builder \
    /home/rust/src/target/x86_64-unknown-linux-musl/release/log-sloth \
    /usr/local/bin/
CMD /usr/local/bin/log-sloth

