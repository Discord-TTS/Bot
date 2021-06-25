FROM python:latest

RUN apt-get update && apt-get install -y ffmpeg

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt uvloop jishaku

COPY . .
CMD ["python3", "main.py"]
