#
# Base stage to re-use for cargo chef
#
FROM rocm/pytorch:rocm4.5.2_ubuntu18.04_py3.8_pytorch_1.10.0 AS base

WORKDIR /src

# Add rustup-installed binaries to the path
ENV PATH=$PATH:/root/.cargo/bin

ENV RUSTBERT_CACHE=/cache

# Tell rust-bert where to find libtorch
ENV LIBTORCH=/opt/conda/lib/python3.8/site-packages/torch
ENV LD_LIBRARY_PATH=${LIBTORCH}/lib:$LD_LIBRARY_PATH

RUN apt-get update && apt-get install -y \
	libssl-dev

# Building layers on top of the rocm image is quite slow, so putting the entrypoint here saves ~30
# seconds per build
ENTRYPOINT ["/bin/gpt2_server"]

# Install rust
RUN set -ex \
	&& curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs > /rustup.sh \
	&& echo 'a3cb081f88a6789d104518b30d4aa410009cd08c3822a1226991d6cf0442a0f8 /rustup.sh' | sha256sum -c \
	&& sh /rustup.sh -y --default-toolchain none \
	&& rustup default 1.58.0

COPY . .
RUN cargo install --path server --root /
