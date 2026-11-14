#!/usr/bin/env bash
# The Ayati cloud test cycle: build, deploy, test, save evidence, delete.
# Data carries over: down snapshots the database and keeps the files bucket.
# Settings come from deploy/aws/.env.cloud (copy cloud.env.example); see README.md.
#
#   env.sh unlock CODE          deployer session from an MFA code (8 hours)
#   env.sh foundation [--with-certificate]
#   env.sh secret workos-api-key|model-api-key FILE
#   env.sh worker-tls           new worker-control CA, certificate and key
#   env.sh image                build, push and remember the image digest
#   env.sh up [--fresh]         create the environment (restoring the last snapshot)
#   env.sh deploy               roll the services to the remembered image
#   env.sh setup                migrate and grant (also part of up and deploy)
#   env.sh status | check | logs [MINUTES] | cost [DAYS]
#   env.sh down [--discard]     save logs, snapshot the database, delete the paid parts
#   env.sh kept                 list the snapshots and files kept between windows
#   env.sh forget [--yes]       delete everything kept (next window starts empty)
#   env.sh verify-clean         list anything temporary that still exists
#   env.sh mail [DOMAIN]        agent email: the permanent mail stack (mail.sh)
# The fast cycle (cycle.sh): park | wake | ship [--full|--rolling] [--dirty] |
#   smoke | db-capacity | reset --yes. Park between sessions instead of down; wake
#   reuses the database; reset empties a test environment's data in place.
set -Eeuo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
settings="${AYATI_CLOUD_SETTINGS:-$here/.env.cloud}"
[ -r "$settings" ] || { echo "Missing $settings (copy cloud.env.example)" >&2; exit 2; }
# shellcheck source=cloud.env.example
. "$settings"
: "${AYATI_AWS_ACCOUNT:?}" "${AYATI_ENV:?}" "${AYATI_DOMAIN:?}" "${AYATI_HOSTNAME:?}"
export AWS_REGION=ap-south-1 AWS_PAGER=''
state="${XDG_STATE_HOME:-$HOME/.local/state}/ayati/aws/$AYATI_ENV"
mkdir -p "$state"
mfa="arn:aws:iam::$AYATI_AWS_ACCOUNT:mfa/${AYATI_AWS_CLI_USER:-cuckoo-cli}"
deployer="arn:aws:iam::$AYATI_AWS_ACCOUNT:role/ayati-access-deployer"
stacks=(network data roles services)
snapshot_prefix="$AYATI_ENV-keep-"
parked_file="$state/parked"   # present while parked (cycle.sh)
kept_snapshots=2

say() { printf '\n== %s\n' "$*"; }
deploy_aws() { aws --profile ayati-deployer "$@"; }
observe_aws() { aws --profile "${AYATI_OBSERVER_PROFILE:-ayati-observer}" "$@"; }
stack() { echo "$AYATI_ENV-$1"; }
output() { deploy_aws cloudformation describe-stacks --stack-name "$1" \
    --query "Stacks[0].Outputs[?OutputKey=='$2'].OutputValue" --output text; }
secret_arn() { deploy_aws secretsmanager describe-secret --secret-id "ayati/$1" --query ARN --output text; }

require_session() {
    deploy_aws sts get-caller-identity >/dev/null 2>&1 \
        || { echo "No deployer session. Run: env.sh unlock <MFA code>" >&2; exit 1; }
}

deploy_stack() {  # deploy_stack <name> <template> [Key=Value...]
    local name=$1 template=$2; shift 2
    say "Stack $name"
    deploy_aws cloudformation deploy --stack-name "$name" --template-file "$here/stacks/$template" \
        --no-fail-on-empty-changeset \
        --tags "ayati:environment=$AYATI_ENV" "ayati:layer=${layer:-temporary}" \
        --parameter-overrides ExpectedAccountId="$AYATI_AWS_ACCOUNT" "$@"
}

unlock() {
    local code=${1:?MFA code}
    local creds
    creds=$(aws sts assume-role --role-arn "$deployer" --role-session-name "ayati-$(whoami)-$(date +%H%M)" \
        --serial-number "$mfa" --token-code "$code" --duration-seconds 28800 \
        --query Credentials --output json)
    aws configure set aws_access_key_id "$(jq -r .AccessKeyId <<<"$creds")" --profile ayati-deployer
    aws configure set aws_secret_access_key "$(jq -r .SecretAccessKey <<<"$creds")" --profile ayati-deployer
    aws configure set aws_session_token "$(jq -r .SessionToken <<<"$creds")" --profile ayati-deployer
    aws configure set region ap-south-1 --profile ayati-deployer
    echo "Deployer session until $(jq -r .Expiration <<<"$creds")."
}

foundation() {
    : "${AYATI_BUDGET_EMAIL:?Set AYATI_BUDGET_EMAIL}"
    local certificate=false
    [ "${1:-}" = --with-certificate ] && certificate=true
    layer=permanent deploy_stack ayati-foundation foundation.yaml Domain="$AYATI_DOMAIN" \
        BudgetEmail="$AYATI_BUDGET_EMAIL" WithCertificate="$certificate"
    say "NS records for $AYATI_DOMAIN (added once at the registrar; they never change)"
    output ayati-foundation NameServers | tr ',' '\n'
}

put_secret() {  # put_secret <name> <value-file>
    if deploy_aws secretsmanager describe-secret --secret-id "ayati/$1" >/dev/null 2>&1; then
        deploy_aws secretsmanager put-secret-value --secret-id "ayati/$1" --secret-string "file://$2" >/dev/null
    else
        deploy_aws secretsmanager create-secret --name "ayati/$1" --secret-string "file://$2" \
            --tags Key=ayati:layer,Value=permanent >/dev/null
    fi
    echo "Stored ayati/$1."
}

secret() {
    case "${1:-}" in workos-api-key|model-api-key) ;; *) echo "Unknown secret" >&2; exit 2 ;; esac
    [ -r "${2:-}" ] || { echo "Unreadable value file" >&2; exit 2; }
    put_secret "$1" "$2"
}

# Executors trust only this private CA; the controller holds the key (worker-control.md).
worker_tls() {
    local work
    work=$(mktemp -d); trap 'rm -rf "$work"' RETURN
    "$root/tools/worker-control-certs.sh" "$work/tls" worker-control.ayati.internal >/dev/null
    jq -n --arg key "$(openssl rand -base64 32 | tr '+/' '-_' | tr -d '=')" \
        --rawfile certificate "$work/tls/server.pem" --rawfile private_key "$work/tls/server.key" \
        '{key: $key, certificate: $certificate, private_key: $private_key}' > "$work/server.json"
    put_secret worker-control-server "$work/server.json"
    put_secret worker-control-ca "$work/tls/ca.pem"
}

image() {
    local release repo digest
    release=$(git -C "$root" rev-parse --short=12 HEAD)
    [ -z "$(git -C "$root" status --porcelain --untracked-files=no)" ] \
        || echo "Warning: uncommitted changes are included in $release" >&2
    repo=$(output ayati-foundation RepositoryUri)
    say "Build $release"
    # Keep BUILD_JOBS fixed: a new value discards the cached Goose build (about 10 minutes).
    docker build --platform linux/amd64 --build-arg AYATI_RELEASE="$release" \
        --build-arg BUILD_JOBS="${AYATI_BUILD_JOBS:-8}" -t "$repo:$release" "$root"
    deploy_aws ecr get-login-password | docker login --username AWS --password-stdin "${repo%%/*}"
    if ! deploy_aws ecr describe-images --repository-name ayati/runtime --image-ids imageTag="$release" >/dev/null 2>&1; then
        docker push "$repo:$release"
    fi
    digest=$(deploy_aws ecr describe-images --repository-name ayati/runtime --image-ids imageTag="$release" \
        --query 'imageDetails[0].imageDigest' --output text)
    echo "$repo@$digest" > "$state/image"
    git -C "$root" rev-parse HEAD > "$state/image-commit"
    echo "Image $repo@$digest"
    mirror_psql "$repo"
}

# The setup task's psql comes from this registry: tasks pull only through the
# VPC's S3 endpoint, which admits ECR layers and the environment's bucket.
mirror_psql() {
    local repo=$1 source=public.ecr.aws/docker/library/postgres:18-alpine tag digest
    docker pull --platform linux/amd64 "$source" >/dev/null
    tag="psql-$(docker image inspect --format '{{.Id}}' "$source" | cut -c8-19)"
    if ! deploy_aws ecr describe-images --repository-name ayati/runtime --image-ids imageTag="$tag" >/dev/null 2>&1; then
        docker tag "$source" "$repo:$tag"
        docker push "$repo:$tag" >/dev/null
    fi
    digest=$(deploy_aws ecr describe-images --repository-name ayati/runtime --image-ids imageTag="$tag" \
        --query 'imageDetails[0].imageDigest' --output text)
    echo "$repo@$digest" > "$state/psql-image"
    echo "psql $repo@$digest ($tag)"
}

services() {  # services <controllers> <executors>
    : "${AYATI_WORKOS_CLIENT_ID:?}" "${AYATI_MODEL:?}"
    # Optional voice-note transcription needs its provider's address too.
    [ -z "${AYATI_TRANSCRIPTION_MODEL:-}" ] || : "${AYATI_TRANSCRIPTION_BASE_URL:?needed with AYATI_TRANSCRIPTION_MODEL}"
    local uri
    uri=$(cat "$state/image" 2>/dev/null) || { echo "Run env.sh image first" >&2; exit 1; }
    deploy_stack "$(stack services)" services.yaml Environment="$AYATI_ENV" ImageUri="$uri" \
        PsqlImage="$(cat "$state/psql-image")" \
        Hostname="$AYATI_HOSTNAME" WorkosClientId="$AYATI_WORKOS_CLIENT_ID" Model="$AYATI_MODEL" \
        ModelProvider="${AYATI_MODEL_PROVIDER:-openai}" ControllerCount="$1" ExecutorCount="$2" \
        ControllerCpu="${AYATI_CONTROLLER_CPU:-256}" ControllerMemory="${AYATI_CONTROLLER_MEMORY:-512}" \
        ExecutorCpu="${AYATI_EXECUTOR_CPU:-512}" ExecutorMemory="${AYATI_EXECUTOR_MEMORY:-1024}" \
        PlatformAdmins="${AYATI_PLATFORM_ADMINS:-}" \
        TranscriptionModel="${AYATI_TRANSCRIPTION_MODEL:-}" TranscriptionBaseUrl="${AYATI_TRANSCRIPTION_BASE_URL:-}" \
        WorkosSecretArn="$(secret_arn workos-api-key)" ModelSecretArn="$(secret_arn model-api-key)" \
        WorkerServerSecretArn="$(secret_arn worker-control-server)" \
        WorkerCaSecretArn="$(secret_arn worker-control-ca)" \
        MailSettings="$(mail_settings)"
}

# The services' current tasks. ECS reports a service at zero while its old tasks
# still drain (AYATI_DRAIN_SECONDS, then the stop timeout), and those tasks still
# hold database connections, so callers list them first and wait for them.
service_tasks() {
    local service
    for service in controller executor; do
        deploy_aws ecs list-tasks --cluster "$AYATI_ENV" --service-name "$service" \
            --query 'taskArns' --output text 2>/dev/null | tr '\t' '\n' | grep '^arn:' || true
    done
}
wait_stopped() {  # wait_stopped "<task ARNs, one per line>"
    [ -n "$1" ] || return 0
    say "Waiting for $(wc -l <<<"$1") old task(s) to stop"
    # shellcheck disable=SC2046  # one ARN per word
    deploy_aws ecs wait tasks-stopped --cluster "$AYATI_ENV" --tasks $(tr '\n' ' ' <<<"$1")
}
# Scales both services to zero and returns once their old tasks have stopped.
stop_services() {
    local tasks
    tasks=$(service_tasks)
    services 0 0
    wait_stopped "$tasks"
}

# Runs migrations as the owner, then grants the runtime role; fails on any non-zero exit.
# An optional ECS overrides document replaces a container's command (reset uses it).
setup() {
    local cluster task subnets group codes overrides=${1:-}
    cluster=$AYATI_ENV
    subnets=$(output "$(stack network)" AppSubnets)
    group=$(output "$(stack network)" ControllerGroup)
    say "Database setup"
    task=$(deploy_aws ecs run-task --cluster "$cluster" --launch-type FARGATE \
        --task-definition "$(output "$(stack services)" SetupTaskDefinition)" \
        --network-configuration "awsvpcConfiguration={subnets=[$subnets],securityGroups=[$group],assignPublicIp=DISABLED}" \
        ${overrides:+--overrides "$overrides"} --query 'tasks[0].taskArn' --output text) || task=''
    # Each step is checked here: callers may run setup where set -e does not apply.
    case "$task" in arn:*) ;; *) echo "Setup task did not start" >&2; return 1 ;; esac
    deploy_aws ecs wait tasks-stopped --cluster "$cluster" --tasks "$task" || { echo "Setup task wait failed" >&2; return 1; }
    codes=$(deploy_aws ecs describe-tasks --cluster "$cluster" --tasks "$task" \
        --query 'tasks[0].containers[].[name,exitCode]' --output text) || codes=''
    echo "$codes"
    # Every container must report exit code 0; a missing code is a failure too.
    if [ -z "$codes" ] || awk '$2 != "0" {bad=1} END {exit !bad}' <<<"$codes"; then
        echo "Setup failed; see env.sh logs" >&2; return 1
    fi
}

wait_stable() {
    say "Waiting for services"
    deploy_aws ecs wait services-stable --cluster "$AYATI_ENV" --services controller executor
    check
}

# A failed up or deploy leaves a paid environment behind; say so plainly.
still_running() {
    trap - ERR
    printf '\nThe environment is still running and costs money by the hour.\n' >&2
    printf 'Fix the problem and run env.sh deploy, or delete it with env.sh down.\n' >&2
}

# Newest available snapshot this environment kept, or nothing.
latest_snapshot() {
    deploy_aws rds describe-db-cluster-snapshots --snapshot-type manual \
        --query "reverse(sort_by(DBClusterSnapshots[?starts_with(DBClusterSnapshotIdentifier,'$snapshot_prefix') && Status=='available'],&SnapshotCreateTime))[0].DBClusterSnapshotIdentifier" \
        --output text | grep -v '^None$' || true
}

# The data stack must keep the snapshot it was created from: a different value
# replaces the database (see data.yaml). A rerun of up reuses the stack's value.
data_exists() { deploy_aws cloudformation describe-stacks --stack-name "$(stack data)" >/dev/null 2>&1; }
restore_from() {
    local fresh=$1 current
    if data_exists; then
        # A failed read must stop up: an empty value would replace a restored database.
        current=$(deploy_aws cloudformation describe-stacks --stack-name "$(stack data)" \
            --query "Stacks[0].Parameters[?ParameterKey=='SnapshotIdentifier'].ParameterValue" --output text) \
            || { echo "Cannot read the database stack's snapshot value; stopping." >&2; return 1; }
        [ "$current" = None ] || echo "$current"
    elif [ "$fresh" = false ]; then
        latest_snapshot
    fi
}

up() {
    require_session
    local fresh=false snapshot
    case "${1:-}" in --fresh) fresh=true ;; '') ;; *) echo "Usage: env.sh up [--fresh]" >&2; exit 2 ;; esac
    trap still_running ERR
    local started; started=$(date -u +%s)
    echo "$started" > "$state/started"
    layer=kept deploy_stack "$(stack keep)" keep.yaml Environment="$AYATI_ENV"
    snapshot=$(restore_from "$fresh")
    if [ -n "$snapshot" ]; then
        say "Restoring the database from $snapshot"
    else
        say "Starting with an empty database"
        # Files from earlier windows would point nowhere; only on the first run of up.
        if [ "$fresh" = true ] && ! data_exists; then empty_bucket "$(output "$(stack keep)" BucketName)"; fi
    fi
    deploy_stack "$(stack network)" network.yaml Environment="$AYATI_ENV" \
        ZoneA=ap-south-1a ZoneB=ap-south-1b WithNat=true
    # MinCapacity above 0 also resumes a parked (paused) Aurora before setup connects.
    deploy_stack "$(stack data)" data.yaml Environment="$AYATI_ENV" SnapshotIdentifier="$snapshot" \
        MinCapacity=0.5
    deploy_stack "$(stack roles)" roles.yaml Environment="$AYATI_ENV" --capabilities CAPABILITY_NAMED_IAM
    stop_services
    mark_deploy_start
    # Migrations connect with a 10 s timeout; a resuming Aurora can take longer once.
    setup || { echo "Setup failed; retrying once in case the database was resuming." >&2; setup; }
    services "${AYATI_CONTROLLERS:-1}" "${AYATI_EXECUTORS:-1}"
    wait_stable
    record_deployed
    rm -f "$parked_file"
    echo "Up in $(( ($(date -u +%s) - started) / 60 )) minutes: https://$AYATI_HOSTNAME"
}

# Controllers accept only the exact schema they were built for, so a new image
# stops the old services, migrates, then starts again (a short outage in test).
deploy() {
    require_session
    trap still_running ERR
    stop_services
    mark_deploy_start
    setup
    services "${AYATI_CONTROLLERS:-1}" "${AYATI_EXECUTORS:-1}"
    wait_stable
    record_deployed
}

check() {
    say "Health"
    curl -fsS --max-time 10 "https://$AYATI_HOSTNAME/health/ready" && echo
    observe_aws ecs describe-services --cluster "$AYATI_ENV" --services controller executor \
        --query 'services[].[serviceName,desiredCount,runningCount,deployments[0].rolloutState]' --output table
}

status() {
    observe_aws cloudformation describe-stacks \
        --query "Stacks[?starts_with(StackName,'ayati')].[StackName,StackStatus]" --output table
}

logs() {
    local minutes=${1:-180} out
    out="$state/windows/$(date -u +%Y%m%dT%H%MZ)"
    mkdir -p "$out"
    for group in controller executor setup; do
        observe_aws logs filter-log-events --log-group-name "/ayati/$AYATI_ENV/$group" \
            --start-time $(( ($(date -u +%s) - minutes * 60) * 1000 )) \
            --query 'events[].message' --output text 2>/dev/null | tr '\t' '\n' > "$out/$group.log" || true
    done
    [ -f "$state/started" ] && echo "started $(date -u -d "@$(cat "$state/started")")" > "$out/window.txt"
    cp "$state/image" "$out/" 2>/dev/null || true
    echo "Saved logs to $out"
}

empty_bucket() {
    local bucket=$1 batch
    deploy_aws s3api head-bucket --bucket "$bucket" >/dev/null 2>&1 || return 0
    while :; do
        batch=$(deploy_aws s3api list-object-versions --bucket "$bucket" --max-items 500 --output json \
            --query '{Objects: [Versions, DeleteMarkers][][].{Key: Key, VersionId: VersionId}, Quiet: `true`}')
        [ "$(jq '.Objects | length' <<<"$batch")" -gt 0 ] || break
        deploy_aws s3api delete-objects --bucket "$bucket" --delete "$batch" >/dev/null
    done
}

# Stops writers first so the snapshot holds the window's final state, then keeps
# the newest few snapshots. Each is billed as storage (a few rupees a month here).
keep_database() {
    local cluster="$AYATI_ENV-core" name
    deploy_aws rds describe-db-clusters --db-cluster-identifier "$cluster" >/dev/null 2>&1 \
        || { echo "No database to keep."; return 0; }
    say "Stopping services before the snapshot"
    local tasks
    tasks=$(service_tasks)
    deploy_aws ecs update-service --cluster "$AYATI_ENV" --service controller --desired-count 0 >/dev/null 2>&1 || true
    deploy_aws ecs update-service --cluster "$AYATI_ENV" --service executor --desired-count 0 >/dev/null 2>&1 || true
    wait_stopped "$tasks" || true
    name="$snapshot_prefix$(date -u +%Y%m%d-%H%M)"
    say "Snapshot $name"
    deploy_aws rds create-db-cluster-snapshot --db-cluster-identifier "$cluster" \
        --db-cluster-snapshot-identifier "$name" --tags Key=ayati:layer,Value=kept >/dev/null
    deploy_aws rds wait db-cluster-snapshot-available --db-cluster-snapshot-identifier "$name"
    deploy_aws rds describe-db-cluster-snapshots --snapshot-type manual \
        --query "reverse(sort_by(DBClusterSnapshots[?starts_with(DBClusterSnapshotIdentifier,'$snapshot_prefix')],&SnapshotCreateTime))[$kept_snapshots:].DBClusterSnapshotIdentifier" \
        --output text | tr '\t' '\n' | while read -r old; do
            [ -n "$old" ] && [ "$old" != None ] || continue
            echo "Removing older snapshot $old"
            deploy_aws rds delete-db-cluster-snapshot --db-cluster-snapshot-identifier "$old" >/dev/null
        done
}

down() {
    require_session
    case "${1:-}" in --discard|'') ;; *) echo "Usage: env.sh down [--discard]" >&2; exit 2 ;; esac
    logs || true
    if [ "${1:-}" = --discard ]; then
        echo "Not keeping this window's database (--discard)."
    else
        trap 'printf "\nThe snapshot failed, so nothing was deleted; the environment still costs money.\nRetry env.sh down, or delete without keeping: env.sh down --discard\n" >&2' ERR
        keep_database
        trap - ERR
    fi
    for (( i=${#stacks[@]}-1; i>=0; i-- )); do
        say "Delete $(stack "${stacks[$i]}")"
        deploy_aws cloudformation delete-stack --stack-name "$(stack "${stacks[$i]}")"
        deploy_aws cloudformation wait stack-delete-complete --stack-name "$(stack "${stacks[$i]}")"
    done
    if [ -f "$state/started" ]; then
        echo "Window lasted $(( ($(date -u +%s) - $(cat "$state/started")) / 60 )) minutes."
        rm -f "$state/started"
    fi
    rm -f "$parked_file"
    verify_clean
}

# Tags can lag deletion, so billable services are also listed directly.
# While parked, the network (without NAT), roles and the paused database stay on purpose.
verify_clean() {
    say "Temporary resources still present"
    local found=0 line name allowed=''
    if [ -f "$parked_file" ]; then
        allowed="$(stack network) $(stack data) $(stack roles) $AYATI_ENV-core"
        echo "Parked on purpose: $allowed"
    fi
    while read -r line; do
        for name in $line; do
            case " $allowed " in *" $name "*) continue ;; esac
            echo "$name"; found=1
        done
    done < <(
        observe_aws cloudformation describe-stacks --query "Stacks[?starts_with(StackName,'$AYATI_ENV-') && StackName!='$(stack keep)'].StackName" --output text
        observe_aws rds describe-db-clusters --query "DBClusters[?starts_with(DBClusterIdentifier,'$AYATI_ENV')].DBClusterIdentifier" --output text
        observe_aws ec2 describe-nat-gateways --filter Name=state,Values=pending,available,deleting \
            --query 'NatGateways[].NatGatewayId' --output text
        observe_aws ec2 describe-addresses --query 'Addresses[].PublicIp' --output text
        observe_aws elbv2 describe-load-balancers --query 'LoadBalancers[].LoadBalancerName' --output text
        observe_aws ecs list-clusters --query 'clusterArns' --output text
        observe_aws s3api list-buckets --query "Buckets[?starts_with(Name,'$AYATI_ENV-') && Name!='$AYATI_ENV-$AYATI_AWS_ACCOUNT-ap-south-1-files'].Name" --output text
    )
    [ "$found" = 0 ] && echo "None. Only the permanent layer and the kept data remain."
    kept
    return "$found"
}

kept() {
    say "Kept between windows"
    observe_aws rds describe-db-cluster-snapshots --snapshot-type manual \
        --query "DBClusterSnapshots[?starts_with(DBClusterSnapshotIdentifier,'$snapshot_prefix')].[DBClusterSnapshotIdentifier,Status,AllocatedStorage]" \
        --output text | awk -F'\t' 'NF {printf "snapshot  %s  %s  %s GiB\n", $1, $2, $3}'
    observe_aws s3api list-buckets --query "Buckets[?Name=='$AYATI_ENV-$AYATI_AWS_ACCOUNT-ap-south-1-files'].Name" \
        --output text | awk 'NF {print "files     " $1}'
}

# Removes the kept snapshots, files and database passwords; the permanent layer stays.
forget() {
    require_session
    if [ "${1:-}" != --yes ]; then
        kept
        echo; echo "env.sh forget --yes deletes all of the above; the next window starts empty." >&2
        return 1
    fi
    deploy_aws cloudformation describe-stacks --stack-name "$(stack data)" >/dev/null 2>&1 \
        && { echo "The environment is still up. Run env.sh down first." >&2; exit 1; }
    deploy_aws rds describe-db-cluster-snapshots --snapshot-type manual \
        --query "DBClusterSnapshots[?starts_with(DBClusterSnapshotIdentifier,'$snapshot_prefix')].DBClusterSnapshotIdentifier" \
        --output text | tr '\t' '\n' | while read -r name; do
            [ -n "$name" ] || continue
            echo "Deleting snapshot $name"
            deploy_aws rds delete-db-cluster-snapshot --db-cluster-snapshot-identifier "$name" >/dev/null
        done
    empty_bucket "$AYATI_ENV-$AYATI_AWS_ACCOUNT-ap-south-1-files"
    say "Delete $(stack keep)"
    deploy_aws cloudformation delete-stack --stack-name "$(stack keep)"
    deploy_aws cloudformation wait stack-delete-complete --stack-name "$(stack keep)"
    echo "Forgotten. The next env.sh up starts with an empty database."
}

cost() {
    local days=${1:-14}
    observe_aws ce get-cost-and-usage --region us-east-1 --granularity DAILY --metrics UnblendedCost \
        --time-period "Start=$(date -u -d "-$days days" +%F),End=$(date -u -d tomorrow +%F)" \
        --group-by Type=DIMENSION,Key=SERVICE \
        --query 'ResultsByTime[].Groups[].[Keys[0],Metrics.UnblendedCost.Amount]' --output text \
        | awk -F'\t' '{sum[$1]+=$2; total+=$2}
            END {for (s in sum) if (sum[s] >= 0.01) printf "%8.2f  %s\n", sum[s], s
                 printf "%8.2f  total (Cost Explorer lags about a day)\n", total}' | sort -rn
}

# shellcheck source=cycle.sh
. "$here/cycle.sh"
# shellcheck source=mail.sh
. "$here/mail.sh"

command=${1:-}; shift || true
case "$command" in
    unlock) unlock "$@" ;;
    foundation) require_session; foundation "$@" ;;
    secret) require_session; secret "$@" ;;
    worker-tls) require_session; worker_tls ;;
    image) require_session; image ;;
    up) up "$@" ;;
    deploy) deploy ;;
    setup) require_session; setup ;;
    status) status ;;
    check) check ;;
    logs) logs "$@" ;;
    cost) cost "$@" ;;
    down) down "$@" ;;
    kept) kept ;;
    forget) forget "$@" ;;
    verify-clean) verify_clean ;;
    park) park ;;
    wake) wake ;;
    ship) ship "$@" ;;
    smoke) smoke ;;
    db-capacity) db_capacity ;;
    reset) reset "$@" ;;
    mail) require_session; mail "$@" ;;
    *) sed -n '2,22p' "$0"; exit 2 ;;
esac
