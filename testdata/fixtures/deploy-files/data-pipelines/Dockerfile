FROM python:3.12-slim

ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    PYTHONPATH=/app

WORKDIR /app

# Render's Docker build context is the repo root, even when Root Directory is set.
COPY data-pipelines/requirements-api.txt ./requirements.txt
RUN pip install --no-cache-dir -r requirements.txt

COPY data-pipelines .
COPY tests ./tests

EXPOSE 8000

CMD ["uvicorn", "api.main:app", "--host", "0.0.0.0", "--port", "8000"]
