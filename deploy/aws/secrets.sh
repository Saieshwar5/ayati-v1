# shellcheck shell=bash disable=SC2154  # helpers and paths come from env.sh
# Keys in Secrets Manager, sourced by env.sh. Values are read from private files
# and never printed; the deployer role can store them but not read them back.
#
#   secret NAME FILE     store one key: workos-api-key, model-api-key (required),
#                        e2b-api-key, composio-api-key, exa-api-key (optional)
#   secret secrets-key   make the sealing key, once
#   secrets              which keys are stored
#   worker-tls           new worker-control CA, certificate and key

put_secret() {  # put_secret <name> <value-file>
    if deploy_aws secretsmanager describe-secret --secret-id "ayati/$1" >/dev/null 2>&1; then
        deploy_aws secretsmanager put-secret-value --secret-id "ayati/$1" --secret-string "file://$2" >/dev/null
    else
        deploy_aws secretsmanager create-secret --name "ayati/$1" --secret-string "file://$2" \
            --tags Key=ayati:layer,Value=permanent >/dev/null
    fi
    echo "Stored ayati/$1."
}

# Empty when the secret was never stored: the services template then leaves that feature off.
optional_arn() {
    deploy_aws secretsmanager describe-secret --secret-id "ayati/$1" --query ARN --output text 2>/dev/null || true
}

# The key that seals stored secrets (Composio's event signing secret, keys people
# save for connected systems). Made once: a new one could not open what is stored.
secrets_key() {
    if [ -n "$(optional_arn secrets-key)" ]; then
        echo "ayati/secrets-key exists and is kept: replacing it would lock stored secrets." >&2; exit 1
    fi
    local work
    work=$(mktemp -d); trap 'rm -rf "$work"; trap - RETURN' RETURN
    (umask 077; openssl rand -hex 32 > "$work/key")
    put_secret secrets-key "$work/key"
}

secret() {
    case "${1:-}" in
        workos-api-key|model-api-key|e2b-api-key|composio-api-key|exa-api-key) ;;
        secrets-key) secrets_key; return ;;
        *) echo "Unknown secret; see env.sh secrets" >&2; exit 2 ;;
    esac
    [ -r "${2:-}" ] || { echo "Unreadable value file" >&2; exit 2; }
    # One line, no spaces: a pasted key with a stray newline or quote fails here, not in the cloud.
    [ "$(tr -d '\n' < "$2" | wc -w)" = 1 ] && [ "$(wc -l < "$2")" -le 1 ] \
        || { echo "The file must hold the key alone, on one line" >&2; exit 2; }
    put_secret "$1" "$2"
}

# Which keys are stored, never their values.
secrets() {
    local name
    for name in workos-api-key model-api-key worker-control-server worker-control-ca \
        e2b-api-key composio-api-key exa-api-key secrets-key; do
        if [ -n "$(optional_arn "$name")" ]; then echo "stored   ayati/$name"; else echo "missing  ayati/$name"; fi
    done
    echo "Required: the first four. Optional: e2b (agent computers), composio (app connections),"
    echo "exa (web search), secrets-key (app events and keys for connected systems)."
}

# Executors trust only this private CA; the controller holds the key (worker-control.md).
worker_tls() {
    local work
    work=$(mktemp -d); trap 'rm -rf "$work"; trap - RETURN' RETURN
    "$root/tools/worker-control-certs.sh" "$work/tls" worker-control.ayati.internal >/dev/null
    jq -n --arg key "$(openssl rand -base64 32 | tr '+/' '-_' | tr -d '=')" \
        --rawfile certificate "$work/tls/server.pem" --rawfile private_key "$work/tls/server.key" \
        '{key: $key, certificate: $certificate, private_key: $private_key}' > "$work/server.json"
    put_secret worker-control-server "$work/server.json"
    put_secret worker-control-ca "$work/tls/ca.pem"
}
