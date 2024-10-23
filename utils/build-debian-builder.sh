#! /usr/bin/env bash

CONTAINER=revive-builder-debian-x86
VERSION=latest
DOCKERFILE=revive-builder-debian.dockerfile

docker build --rm -t ${CONTAINER}:${VERSION} -f ${DOCKERFILE} $@
