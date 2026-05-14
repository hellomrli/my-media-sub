FROM python:3.12-slim

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    BOT_PORT=8787

WORKDIR /app

COPY requirements.txt ./
RUN pip install --no-cache-dir -r requirements.txt

COPY src ./src
COPY README.md ./README.md

EXPOSE 8787

CMD ["sh", "-c", "uvicorn src.main:app --host 0.0.0.0 --port ${BOT_PORT}"]
