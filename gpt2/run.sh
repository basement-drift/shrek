#!/bin/bash
set -euxo pipefail

docker build -t gpt2_server .

docker run -it \
    --rm \
    --device=/dev/kfd \
    --device=/dev/dri \
    --network=host \
    --group-add video \
    --cap-add=SYS_PTRACE \
    --security-opt seccomp=unconfined \
    --privileged \
    --init \
    -v "${GPT2_CACHE_DIR}:/cache" \
    -e APP_PORT=50080 \
    -e HSA_ENABLE_SDMA=${HSA_ENABLE_SDMA:-1} \
    --name gpt2_server \
    gpt2_server:latest
