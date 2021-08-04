FROM python:latest

RUN apt-get update && apt-get install -y ffmpeg make gcc git

RUN git clone https://github.com/vishnubob/wait-for-it wait-for-it && \
    cd wait-for-it && chmod +x wait-for-it.sh && mv wait-for-it.sh / && \
    cd / && rm -rf wait-for-it

RUN git clone https://github.com/aio-libs/aiohttp aiohttp-git && cd aiohttp-git && \
    git reset --hard 3250c5d75a54e19e2825d0a609f9d9cd4bf62087 && \
    git submodule update --init && make cythonize && cd /

COPY requirements.txt .
RUN pip install --no-cache-dir -U -r requirements.txt uvloop jishaku
RUN pip install --no-cache-dir -U ./aiohttp-git[speedups] && rm -rf aiohttp-git

COPY . .
CMD ["python3", "-u", "main.py"]
