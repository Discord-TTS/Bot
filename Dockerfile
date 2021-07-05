FROM python:latest

RUN apt-get update && apt-get install -y ffmpeg

RUN git clone https://github.com/vishnubob/wait-for-it wait-for-it && \
    cd wait-for-it && chmod +x wait-for-it.sh && mv wait-for-it.sh / && \
    cd / && rm -rf wait-for-it

COPY requirements.txt .
RUN pip install --no-cache-dir -U -r requirements.txt uvloop jishaku

COPY . .
CMD ["python3", "-u", "main.py"]
