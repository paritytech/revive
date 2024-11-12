#! /usr/bin/env bash

CONTAINER=revive-builder-alpine-x86
VERSION=latest
DOCKERFILE=revive-builder-alpine.dockerfile

docker build --rm -t ${CONTAINER}:${VERSION} -f ${DOCKERFILE} $@
