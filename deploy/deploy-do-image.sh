#!/bin/bash

docker-compose -f server/docker-compose.yml build
docker tag mailclerk-server:amd64 registry.digitalocean.com/mgrog-cr/mailclerk-server:amd64
docker push registry.digitalocean.com/mgrog-cr/mailclerk-server:amd64