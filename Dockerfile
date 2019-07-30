FROM python:3.7-alpine

RUN pip install slackbot

WORKDIR /src

COPY requirements.txt .
RUN pip install -r requirements.txt

COPY ./ .

ENTRYPOINT ["python", "run.py"]
