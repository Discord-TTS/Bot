FROM python:latest

RUN apt-get update && apt-get install -y ffmpeg espeak make gcc git

RUN git clone https://github.com/vishnubob/wait-for-it wait-for-it && \
    cd wait-for-it && chmod +x wait-for-it.sh && mv wait-for-it.sh / && \
    cd / && rm -rf wait-for-it

RUN git clone https://github.com/numediart/MBROLA MBROLA && \
    cd MBROLA && make && mv Bin/mbrola /usr/bin/mbrola && \
    cd / && rm -rf MBROLA

COPY requirements.txt .
RUN pip install --no-cache-dir -U -r requirements.txt uvloop jishaku
RUN python3 -u -m voxpopuli.voice_install --all en

COPY . .
CMD ["python3", "-u", "main.py"]
