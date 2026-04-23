#!/usr/bin/env bash
set -euo pipefail

# Per-service deployment wrapper around `docker compose`.
# Lets you build/start/stop/restart/logs an individual service or all of them.

SERVICES=(catalog-service streaming-service user-service gateway)
ALL_TARGETS=("${SERVICES[@]}" all)

COMPOSE_CMD=${COMPOSE_CMD:-}
if [ -z "$COMPOSE_CMD" ]; then
    if command -v docker >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; then
        COMPOSE_CMD="docker compose"
    elif command -v podman-compose >/dev/null 2>&1; then
        COMPOSE_CMD="podman-compose"
    elif command -v docker-compose >/dev/null 2>&1; then
        COMPOSE_CMD="docker-compose"
    else
        echo "Error: no compose tool found (tried docker compose, podman-compose, docker-compose)." >&2
        echo "Set COMPOSE_CMD to override." >&2
        exit 1
    fi
fi

usage() {
    cat <<EOF
Usage: $(basename "$0") <action> [service]

Actions:
  up        Build and start the service (detached)
  down      Stop and remove the service container
  restart   Restart the service
  build     Build the image without starting
  rebuild   Force rebuild (no cache) and restart
  logs      Follow the service's logs
  status    Show compose ps for the service
  shell     Open an interactive shell in the running container

Services:
  catalog-service, streaming-service, user-service, gateway, all

Examples:
  $(basename "$0") up streaming-service
  $(basename "$0") rebuild gateway
  $(basename "$0") logs catalog-service
  $(basename "$0") up all
EOF
}

if [ $# -lt 1 ]; then
    usage
    exit 1
fi

ACTION=$1
SERVICE=${2:-all}

case "$ACTION" in
    -h|--help|help)
        usage
        exit 0
        ;;
esac

case "$ACTION" in
    up|down|restart|build|rebuild|logs|status|ps|shell) ;;
    *)
        echo "Error: unknown action '$ACTION'." >&2
        usage
        exit 1
        ;;
esac

valid=0
for s in "${ALL_TARGETS[@]}"; do
    if [ "$s" = "$SERVICE" ]; then valid=1; break; fi
done
if [ "$valid" -ne 1 ]; then
    echo "Error: unknown service '$SERVICE'." >&2
    echo "Valid: ${ALL_TARGETS[*]}" >&2
    exit 1
fi

if [ ! -f .env ]; then
    echo "Error: .env not found. Run ./setup.sh first." >&2
    exit 1
fi

# Expand "all" to the full service list for commands that accept multiple names.
expand_targets() {
    if [ "$SERVICE" = "all" ]; then
        printf '%s\n' "${SERVICES[@]}"
    else
        printf '%s\n' "$SERVICE"
    fi
}

case "$ACTION" in
    up)
        # shellcheck disable=SC2046
        $COMPOSE_CMD up --build -d $(expand_targets)
        ;;
    down)
        # `stop` + `rm -f` only affects the named service; `down` without args
        # would tear down the whole project including networks/volumes.
        # shellcheck disable=SC2046
        $COMPOSE_CMD stop $(expand_targets)
        # shellcheck disable=SC2046
        $COMPOSE_CMD rm -f $(expand_targets)
        ;;
    restart)
        # shellcheck disable=SC2046
        $COMPOSE_CMD restart $(expand_targets)
        ;;
    build)
        # shellcheck disable=SC2046
        $COMPOSE_CMD build $(expand_targets)
        ;;
    rebuild)
        # shellcheck disable=SC2046
        $COMPOSE_CMD build --no-cache $(expand_targets)
        # shellcheck disable=SC2046
        $COMPOSE_CMD up -d --force-recreate $(expand_targets)
        ;;
    logs)
        # shellcheck disable=SC2046
        $COMPOSE_CMD logs -f --tail=200 $(expand_targets)
        ;;
    status|ps)
        # shellcheck disable=SC2046
        $COMPOSE_CMD ps $(expand_targets)
        ;;
    shell)
        if [ "$SERVICE" = "all" ]; then
            echo "Error: 'shell' requires a specific service." >&2
            exit 1
        fi
        $COMPOSE_CMD exec "$SERVICE" /bin/bash 2>/dev/null \
            || $COMPOSE_CMD exec "$SERVICE" /bin/sh
        ;;
esac
