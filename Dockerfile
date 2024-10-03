##
## Build stage
##
FROM rust:latest as builder

RUN apt-get update && apt-get install -y cron

WORKDIR /usr/src/yagg

COPY ./yagg .

RUN cargo build

##
## Final stage
##
FROM ubuntu:24.04
WORKDIR /usr/src/yagg

RUN apt-get update && apt-get -y install cron && apt-get clean

COPY --from=builder /usr/src/yagg/target/debug/yagg .

RUN echo "*/5 * * * * cd /usr/src/yagg/ && ./yagg >> /var/log/cron.log 2>&1" > /etc/cron.d/yagg-cron

RUN chmod 0644 /etc/cron.d/yagg-cron && crontab /etc/cron.d/yagg-cron

CMD ["cron", "-f"]
