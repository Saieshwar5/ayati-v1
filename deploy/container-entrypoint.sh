#!/bin/sh
# Fixed secret names only; never eval file contents or print secret values.
set -eu
if [ "${1-}" = bridge ]; then
    shift
    exec /opt/ayati/bridge/bin/python -m aws_bridge "$@"
fi
load_secret() {
    name=$1
    eval 'secret_file=${'"${name}"'_FILE-}'
    eval 'secret_value=${'"${name}"'-}'
    if [ -n "$secret_file" ]; then
        if [ -n "$secret_value" ]; then
            echo "Configure either $name or ${name}_FILE, not both" >&2
            exit 1
        fi
        if [ ! -r "$secret_file" ]; then
            echo "Cannot read ${name}_FILE" >&2
            exit 1
        fi
        secret_value=$(cat -- "$secret_file")
        if [ -z "$secret_value" ]; then
            echo "${name}_FILE is empty" >&2
            exit 1
        fi
        export "$name=$secret_value"
        unset "${name}_FILE"
    fi
}
for name in RELAY_DATABASE_URL RELAY_SIMULATION_DATABASE_URL WORKOS_API_KEY OPENAI_API_KEY FIREWORKS_API_KEY \
    AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN; do
    load_secret "$name"
done
unset secret_file secret_value name
exec /opt/ayati/ayati "$@"
