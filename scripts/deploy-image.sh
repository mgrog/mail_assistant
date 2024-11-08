REGISTRY="mgrog-cr"
IMAGE_TAG="mailclerk-server:latest"

docker-compose -f server/docker-compose.yml build &&
doctl registry login &&
docker tag $IMAGE_TAG registry.digitalocean.com/$REGISTRY/$IMAGE_TAG &&
docker push registry.digitalocean.com/$REGISTRY/$IMAGE_TAG