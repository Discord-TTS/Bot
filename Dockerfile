FROM python:latest

RUN apt-get update && apt-get install -y ffmpeg
RUN curl https://raw.githubusercontent.com/vishnubob/wait-for-it/master/wait-for-it.sh -o wait-for-it.sh
RUN chmod +x wait-for-it.sh

COPY requirements.txt .
RUN pip install --no-cache-dir -U -r requirements.txt uvloop jishaku

COPY . .
CMD ["python3", "-u", "main.py"]
