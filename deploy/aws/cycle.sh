# shellcheck shell=bash disable=SC2154  # settings, state paths and helpers come from env.sh
# The fast cloud cycle (D244), sourced by env.sh: park, wake, ship and smoke.
# Plan: proj-docs/cloud/fast-cycle-plan.md. Uses env.sh's settings and helpers.
#
#   park   save logs, delete the services stack (controllers, executors, load
#          balancer), turn the NAT off and let Aurora pause at 0 ACU
#   wake   up again from the parked state (the database is reused, not restored)
#   ship   image, then a rolling deploy when the schema is unchanged (a stop,
#          migrate and start otherwise), then smoke
#   smoke  readiness, service counts, the running image and new log errors
#   reset  delete every workspace, person, agent, message and file in a test
#          environment, keeping the environment itself (sign-ins at WorkOS stay)


# Called at the start of anything that starts new tasks; smoke reads logs from here.
mark_deploy_start() { date -u +%s > "$state/deploy-started"; }

# The commit whose image is now running, for ship's schema comparison.
record_deployed() {
    cp "$state/image-commit" "$state/deployed-commit" 2>/dev/null || true
}

park() {
    require_session
    data_exists || { echo "Nothing to park: there is no database. Use env.sh up." >&2; exit 1; }
    trap 'printf "\nPark stopped part-way. Run env.sh park again, or env.sh down.\n" >&2' ERR
    logs || true
    say "Delete $(stack services) (controllers, executors, load balancer, DNS name)"
    deploy_aws cloudformation delete-stack --stack-name "$(stack services)"
    deploy_aws cloudformation wait stack-delete-complete --stack-name "$(stack services)"
    deploy_stack "$(stack network)" network.yaml Environment="$AYATI_ENV" \
        ZoneA=ap-south-1a ZoneB=ap-south-1b WithNat=false
    # SnapshotIdentifier is left out on purpose: deploy keeps the stack's existing
    # value, and a different one would replace the database.
    deploy_stack "$(stack data)" data.yaml Environment="$AYATI_ENV" MinCapacity=0
    trap - ERR
    touch "$parked_file"
    if [ -f "$state/started" ]; then
        echo "Window lasted $(( ($(date -u +%s) - $(cat "$state/started")) / 60 )) minutes."
        rm -f "$state/started"
    fi
    verify_clean || true
    echo
    echo "Parked. Aurora pauses after 5 minutes without connections; check it with"
    echo "env.sh db-capacity. Schedules and agent wake-ups do not run while parked."
}

wake() {
    [ -f "$parked_file" ] || echo "Not parked; running up anyway."
    up
    smoke
}

# Newest migration name in a commit's source; equal names mean an equal schema.
newest_migration() {
    git -C "$root" grep -ohE 'm20[0-9]{6}_[0-9]{6}_[a-z0-9_]+' "$1" -- src | sort -u | tail -1
}

# rolling when the running commit has the same schema and the same services
# template (setup grants live there); full otherwise or when unsure.
deploy_mode() {
    local deployed
    deployed=$(cat "$state/deployed-commit" 2>/dev/null) || { echo full; return; }
    git -C "$root" cat-file -e "$deployed^{commit}" 2>/dev/null || { echo full; return; }
    if [ "$(newest_migration "$deployed")" = "$(newest_migration HEAD)" ] \
        && git -C "$root" diff --quiet "$deployed" HEAD -- deploy/aws/stacks/services.yaml; then
        echo rolling
    else
        echo full
    fi
}

ship() {
    local mode=auto dirty=false arg
    for arg in "$@"; do
        case $arg in
            --full) mode=full ;;
            --rolling) mode=rolling ;;
            --dirty) dirty=true ;;
            *) echo "Usage: env.sh ship [--full|--rolling] [--dirty]" >&2; exit 2 ;;
        esac
    done
    require_session
    [ ! -f "$parked_file" ] || { echo "The environment is parked. Run env.sh wake first." >&2; exit 1; }
    # Only what enters the image or the deployment counts (see .dockerignore).
    local inputs=(Dockerfile .dockerignore Cargo.toml Cargo.lock src crates web tools integrations deploy)
    if [ "$dirty" = false ] && [ -n "$(git -C "$root" status --porcelain --untracked-files=no -- "${inputs[@]}")" ]; then
        echo "Uncommitted changes in the image or deployment: commit them first, or pass --dirty." >&2
        git -C "$root" status --short --untracked-files=no -- "${inputs[@]}" >&2
        exit 1
    fi
    local started; started=$(date -u +%s)
    image
    [ "$mode" != auto ] || mode=$(deploy_mode)
    if [ "$mode" = rolling ]; then
        say "Rolling deploy (same schema as the running image)"
        trap still_running ERR
        mark_deploy_start
        services "${AYATI_CONTROLLERS:-1}" "${AYATI_EXECUTORS:-1}"
        wait_stable
        trap - ERR
        record_deployed
    else
        say "Full deploy (schema or setup changed): stop, migrate, start"
        deploy
    fi
    smoke
    echo "Shipped $(cat "$state/image-commit" | cut -c1-12) ($mode) in $(( ($(date -u +%s) - started) / 60 )) minutes."
}

# Prints each check; returns non-zero when readiness, counts or the image are wrong.
smoke_checks() {
    local since_ms=$1 failed=0 line expected images
    say "Readiness"
    if curl -fsS --max-time 10 "https://$AYATI_HOSTNAME/health/ready"; then echo; else echo "NOT READY"; failed=1; fi

    say "Services (name desired running rollout)"
    while read -r line; do
        [ -n "$line" ] || continue
        echo "$line"
        awk '{exit !($2 == $3 && $2 > 0 && $4 == "COMPLETED")}' <<<"$line" || failed=1
    done < <(observe_aws ecs describe-services --cluster "$AYATI_ENV" --services controller executor \
        --query 'services[].[serviceName,desiredCount,runningCount,deployments[0].rolloutState]' --output text)

    say "Running image"
    expected=$(cat "$state/image" 2>/dev/null || echo unknown)
    for service in controller executor; do
        images=$(observe_aws ecs describe-task-definition --task-definition "$(observe_aws ecs describe-services \
            --cluster "$AYATI_ENV" --services "$service" --query 'services[0].taskDefinition' --output text)" \
            --query "taskDefinition.containerDefinitions[?name=='$service'].image" --output text)
        if [ "$images" = "$expected" ]; then echo "$service: expected image"; else echo "$service: $images (expected $expected)"; failed=1; fi
    done

    say "Logs since the deploy started: errors and warnings (ERROR and panics fail smoke)"
    local found
    for group in setup controller executor; do
        found=$(observe_aws logs filter-log-events --log-group-name "/ayati/$AYATI_ENV/$group" --start-time "$since_ms" \
            --filter-pattern '?ERROR ?WARN ?panicked' --query 'events[].message' --output json 2>/dev/null \
            | jq -r '.[] | (fromjson? // {level: "TEXT", message: .}) | "\(.level) \(.message)"' \
            | sort | uniq -c | sort -rn) || true
        [ -n "$found" ] || continue
        # shellcheck disable=SC2001  # prefix every line, not one string
        sed "s/^/$group: /" <<<"$found"
        grep -qE '^ *[0-9]+ (ERROR |TEXT .*panicked)' <<<"$found" && failed=1
    done
    say "Processes started since the deploy (role release)"
    for group in controller executor; do
        observe_aws logs filter-log-events --log-group-name "/ayati/$AYATI_ENV/$group" --start-time "$since_ms" \
            --filter-pattern '{ $.message = "Starting" }' --query 'events[].message' --output json 2>/dev/null \
            | jq -r '.[] | fromjson? | "\(.role) \(.release)"' | sort | uniq -c || true
    done
    return "$failed"
}

smoke() {
    local out since
    out="$state/windows/$(date -u +%Y%m%dT%H%MZ)"
    mkdir -p "$out"
    since=$(cat "$state/deploy-started" 2>/dev/null || echo $(( $(date -u +%s) - 1800 )))
    if smoke_checks "$(( since * 1000 ))" 2>&1 | tee "$out/smoke.txt"; then
        echo; echo "Smoke passed. Saved $out/smoke.txt"
    else
        echo; echo "Smoke FAILED. Saved $out/smoke.txt" >&2
        return 1
    fi
}

# Aurora's capacity over the last 15 minutes; 0 means paused (only storage billed).
db_capacity() {
    observe_aws cloudwatch get-metric-statistics --namespace AWS/RDS --metric-name ServerlessDatabaseCapacity \
        --dimensions Name=DBClusterIdentifier,Value="$AYATI_ENV-core" --statistics Minimum Maximum --period 300 \
        --start-time "$(date -u -d '-15 minutes' +%FT%TZ)" --end-time "$(date -u +%FT%TZ)" \
        --query 'sort_by(Datapoints,&Timestamp)[].[Timestamp,Minimum,Maximum]' --output text
}

# Test environments only: starts the data over without recreating the environment.
reset() {
    if [ "${1:-}" != --yes ]; then
        echo "env.sh reset --yes deletes every workspace, person, agent, message and file in" >&2
        echo "$AYATI_ENV and migrates an empty database. Sign-ins at WorkOS stay." >&2
        exit 2
    fi
    case "$AYATI_ENV" in *prod*) echo "Refusing to reset $AYATI_ENV: production data is never reset." >&2; exit 2 ;; esac
    require_session
    [ ! -f "$parked_file" ] || { echo "The environment is parked. Run env.sh wake first." >&2; exit 1; }
    data_exists || { echo "There is no database to reset." >&2; exit 1; }
    trap still_running ERR
    logs || true
    say "Stopping the services (no connections may stay open)"
    stop_services
    # The grants container connects to the postgres database as the owner; migrate
    # runs first on the old database and changes nothing.
    say "Dropping and recreating the database"
    setup "$(jq -cn '{containerOverrides: [{name: "grants",
        environment: [{name: "PGDATABASE", value: "postgres"}],
        command: ["psql -v ON_ERROR_STOP=1 -c \"DROP DATABASE ayati WITH (FORCE)\" -c \"CREATE DATABASE ayati\" && echo Database recreated."]}]}')"
    say "Emptying the files bucket"
    empty_bucket "$(output "$(stack keep)" BucketName)"
    mark_deploy_start
    setup
    services "${AYATI_CONTROLLERS:-1}" "${AYATI_EXECUTORS:-1}"
    wait_stable
    record_deployed
    trap - ERR
    smoke
    echo "Reset: an empty database at $(newest_migration "$(cat "$state/deployed-commit")"). Sign in again to start."
}
