FROM ubuntu:22.04

RUN apt-get update && apt-get install -y libssl-dev ca-certificates

COPY target/release/ctrl-cidr /ctrl-cidr

ENTRYPOINT ["/ctrl-cidr"]
