.PHONY: build build-env-image push push-perf create deploy login local

REPO := $(shell aws ecr describe-repositories | jq -r '.repositories | map(select(.repositoryName == "log-sloth") | .repositoryUri) | first')
TAG := $(shell date +%Y-%m-%d-%H-%M)

build: # did you run build-env-image recently?
	docker run --rm -it -v $(PWD):/home/rust/src log-sloth-build-env cargo build --release
	cp target/x86_64-unknown-linux-musl/release/log-sloth target/log-sloth
	docker build $(BUILD_FLAGS) -t $(REPO):$(TAG) .

build-env-image:
	docker build -t log-sloth-build-env -f Dockerfile.build-env .

push: build
	docker push $(REPO):$(TAG)

push-perf:
	BUILD_FLAGS="-f Dockerfile.perf" $(MAKE) push

create:
	kubectl create -f docker/log-sloth-deployment.yml

deploy: push
	kubectl set image deploy/log-sloth log-sloth=$(REPO):$(TAG)

login:
	aws ecr get-login --no-include-email | sh

local: build
	docker run --rm -it -p 1516:1516 -e USER=root -e AWS_ACCESS_KEY_ID=$(shell aws configure get aws_access_key_id) -e AWS_SECRET_ACCESS_KEY=$(shell aws configure get aws_secret_access_key) $(REPO):$(TAG) sh
