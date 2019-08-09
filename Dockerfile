FROM python:3.7-alpine

WORKDIR /src

# Install our nltk models early because they cache well
COPY install_nltk.py .
RUN pip install nltk && python install_nltk.py

RUN apk update && apk add \
	libxml2 \
	libxml2-dev \
	libxslt \
	libxslt-dev \
	build-base

# Install this separately, because it builds slowly
RUN pip install readability-lxml

COPY requirements.txt .
RUN pip install -r requirements.txt

COPY ./ .

ENTRYPOINT ["python", "run.py"]
