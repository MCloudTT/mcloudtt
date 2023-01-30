FROM alpine:latest
LABEL Name=mcloudtt Version=0.0.2
COPY target/x86_64-unknown-linux-musl/release/mcloudtt .
COPY certs ./certs
COPY sa.key .
ENV RUST_LOG=debug
ENV RUST_BACKTRACE=1
EXPOSE 1883
CMD ["./mcloudtt"]
