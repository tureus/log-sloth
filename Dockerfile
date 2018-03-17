# Now, we need to build our _real_ Docker container, copying in `using-diesel`.
FROM alpine:latest
ADD docker/apk-repositories /etc/apk/repositories
RUN apk --no-cache add ca-certificates htop sysstat procps
ADD target/log-sloth \
    /usr/local/bin/
CMD /usr/local/bin/log-sloth

