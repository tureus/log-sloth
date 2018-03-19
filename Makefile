.PHONY: build build-env-image push push-perf create deploy login local

REPO := $(shell aws ecr describe-repositories | jq -r '.repositories | map(select(.repositoryName == "log-sloth") | .repositoryUri) | first')
TAG := $(shell date +%Y-%m-%d-%H-%M)

# We need to thread the needle: mount-bind everything from
# here except for the cached build environment. We want to reuse
# the prebuilt libs if Cargo decides they are relevant. Cargo will take care
# of reusing or rebuilding as necessary.
SRC := /home/rust/src
MUSL_BIN := target/x86_64-unknown-linux-musl/release/log-sloth
MUSL_HATCH := target/from_musl/
DASH_V := -v $(PWD)/Cargo.toml:$(SRC)/Cargo.toml -v $(PWD)/Cargo.lock:$(SRC)/Cargo.lock -v $(PWD)/src/:$(SRC)/src/ -v $(PWD)/nom-syslog/:$(SRC)/src/nom-syslog/ -v $(PWD)/$(MUSL_HATCH):$(SRC)/$(MUSL_HATCH)


build: # did you run build-env-image recently?
	mkdir -p $(MUSL_HATCH)
	docker run --rm -it $(DASH_V) log-sloth-build-env bash -c "cargo build --release && mv $(MUSL_BIN) $(MUSL_HATCH)"
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
