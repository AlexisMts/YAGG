# YAGG - Yet Another Gaps Grader

<div align="center">
    <img src="./figures/logo.png" />
</div>

YAGG (aka Yet Another Gaps Grader) is a Rust application designed to periodically check and notify you about new grades posted on gaps. 
The application is packaged as a Docker container, you just have to take 2 minutes to set it up and you're good to go.

When you receive a new grade, the telegram bot will send you a message with the grade, if it was a lab or a course grade, and obviously the course name.

## Getting Started

### Prerequisites

- Docker/Docker Compose
- Telegram bot

### Installation

1. Clone this repository
2. Configure the Dockerfile with the time interval you want to check for new grades

```Dockerfile
RUN echo "*/<YOUR INTERVAL HERE> * * * * cd /usr/src/yagg/target/debug/ && ./yagg >> /var/log/cron.log 2>&1" > /etc/cron.d/yagg-cron
```

3. Configure the environment variables in the `.env` file

```
# path: /yagg/.env

GAPS_USERNAME='<YOUR GAPS USERNAME>'
GAPS_PASSWORD='<YOUR GAPS PASSWORD>'
BOT_TOKEN=<TELEGRAM BOT TOKEN>
CHAT_ID=<TELEGRAM CHAT ID>
```

4. Run the application

```bash
docker-compose up -d --build
```

## Acknowledgments

This project utilizes a parsing algorithm adapted from [AutoGaps](https://github.com/azzen/auto-gaps/tree/master).
Which is also an application to check for new grades on gaps, but it's written in Python.