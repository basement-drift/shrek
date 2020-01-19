# shrek

To run with all plugins enabled, a docker command like the following can be used:
```
docker run \
	-e SLACKBOT_API_TOKEN=<token> \
	-e GIPHY_API_KEY=<key> \
	-e YOUTUBE_API_KEY=<key> \
	-e MARKOV_MODEL_PATH=<path> \
	--device=/dev/kfd \
	--device=/dev/dri \
	--ipc=host \
	--shm-size 16G \
	--group-add video \
	--cap-add=SYS_PTRACE \
	--security-opt seccomp=unconfined \
	shrek
```

The device, IPC, and security options are necessary for GPU tensorflow integration for the GPT2 model.
