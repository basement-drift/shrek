FROM rocm/tensorflow:rocm3.0-tf1.15-python3

WORKDIR /src

RUN apt-get update && apt-get install -y \
	libxml2 \
	libxml2-dev \
	libxslt1.1 \
	libxslt1-dev

# Install this separately, because it builds slowly
RUN pip3 install readability-lxml

COPY requirements.txt .
RUN pip3 install -r requirements.txt

# Install our models separately because they cache well
COPY install_models.py .
ENV GPT_2_MODEL=774M
RUN python3 install_models.py

COPY ./ .

ENTRYPOINT ["python3", "run.py"]
