.PHONY: push deploy local

REPO := $(shell aws ecr describe-repositories | jq -r '.repositories | map(select(.repositoryName == "log-sloth") | .repositoryUri) | first')
TAG := latest

build:
login:
	aws ecr get-login --no-include-email | sh

local: build
	docker run --rm -it -p 1516:1516 -e USER=root -e AWS_ACCESS_KEY_ID=$(shell aws configure get aws_access_key_id) -e AWS_SECRET_ACCESS_KEY=$(shell aws configure get aws_secret_access_key) $(REPO):$(TAG) sh
