.PHONY: push deploy local

REPO := 455567940957.dkr.ecr.us-west-2.amazonaws.com/log-sloth
TAG := latest

login:
	aws ecr get-login --no-include-email | sh

build:
	docker run --rm --user "$(shell id -u)":"$(shell id -g)" -v "$(PWD)":/source/log-sloth -w /source/log-sloth yasuyuky/rust-ssl-static cargo build --release
	docker build -t $(REPO):$(TAG) .

push: build
	docker push $(REPO):$(TAG)

local:
	docker run --rm -it $(REPO):$(TAG) bash