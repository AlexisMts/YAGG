##
## Build stage
##
FROM rust:alpine AS builder

WORKDIR /build/yagg

RUN apk update && apk add libressl-dev musl-dev

COPY yagg .

RUN cargo build --release

##
## Final stage
##
FROM alpine:3.20
WORKDIR /app

ENV GAPS_USERNAME=your_username \
    GAPS_PASSWORD=your_password \
    BOT_TOKEN=telergam_api_key \
    CHAT_ID=telegram_chat_id

COPY docker/root /var/spool/cron/crontabs/

COPY --from=builder /build/yagg/target/release/yagg /app/yagg
RUN chmod +x /app/yagg

CMD ["crond", "-f"]
