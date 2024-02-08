FROM rust:latest

RUN apt-get update && apt-get install -y cron

WORKDIR /usr/src/yagg

COPY ./yagg .

RUN cargo build

RUN echo "*/5 * * * * cd /usr/src/yagg/target/debug/ && ./yagg >> /var/log/cron.log 2>&1" > /etc/cron.d/yagg-cron

RUN chmod 0644 /etc/cron.d/yagg-cron && crontab /etc/cron.d/yagg-cron

CMD ["cron", "-f"]
