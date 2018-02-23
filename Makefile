.PHONY: push deploy local

REPO := $(shell aws ecr describe-repositories | jq -r '.repositories | map(select(.repositoryName == "log-sloth") | .repositoryUri) | first')
TAG := $(shell date +%Y-%m-%d-%H-%M)

build:
	docker build ${BUILD_FLAGS} -t $(REPO):$(TAG) .

push-perf:
	BUILD_FLAGS="-f Dockerfile.perf" $(MAKE) push

push: build
	docker push $(REPO):$(TAG)

create:
	kubectl create -f docker/log-sloth-deployment.yml

deploy: push
	kubectl set image deploy/log-sloth log-sloth=$(REPO):$(TAG)

login:
	aws ecr get-login --no-include-email | sh

local: build
	docker run --rm -it -p 1516:1516 -e USER=root -e AWS_ACCESS_KEY_ID=$(shell aws configure get aws_access_key_id) -e AWS_SECRET_ACCESS_KEY=$(shell aws configure get aws_secret_access_key) $(REPO):$(TAG) sh
