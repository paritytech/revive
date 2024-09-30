#! /usr/bin/env bash

CONTAINER=revive-builder-debian-x86
VERSION=latest

docker run --rm -v $(pwd):$(pwd) -w $(pwd) ${CONTAINER}:${VERSION} $@
