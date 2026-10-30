#!/usr/bin/env bash
# The Ayati cloud test cycle: build, deploy, test, save evidence, delete.
# Settings come from deploy/aws/.env.cloud (copy cloud.env.example); see README.md.
#
#   env.sh unlock CODE          deployer session from an MFA code (8 hours)
#   env.sh foundation [--with-certificate]
#   env.sh secret workos-api-key|model-api-key FILE
#   env.sh worker-tls           new worker-control CA, certificate and key
#   env.sh image                build, push and remember the image digest
#   env.sh up                   create the environment and start services
#   env.sh deploy               roll the services to the remembered image
#   env.sh setup                migrate and grant (also part of up and deploy)
#   env.sh status | check | logs [MINUTES] | cost [DAYS]
#   env.sh down                 save logs, empty the bucket, delete everything
#   env.sh verify-clean         list anything temporary that still exists
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
    docker build --platform linux/amd64 --build-arg AYATI_RELEASE="$release" \
        --build-arg BUILD_JOBS="${AYATI_BUILD_JOBS:-4}" -t "$repo:$release" "$root"
    deploy_aws ecr get-login-password | docker login --username AWS --password-stdin "${repo%%/*}"
    if ! deploy_aws ecr describe-images --repository-name ayati/runtime --image-ids imageTag="$release" >/dev/null 2>&1; then
        docker push "$repo:$release"
    fi
    digest=$(deploy_aws ecr describe-images --repository-name ayati/runtime --image-ids imageTag="$release" \
        --query 'imageDetails[0].imageDigest' --output text)
    echo "$repo@$digest" > "$state/image"
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
    local uri
    uri=$(cat "$state/image" 2>/dev/null) || { echo "Run env.sh image first" >&2; exit 1; }
    deploy_stack "$(stack services)" services.yaml Environment="$AYATI_ENV" ImageUri="$uri" \
        PsqlImage="$(cat "$state/psql-image")" \
        Hostname="$AYATI_HOSTNAME" WorkosClientId="$AYATI_WORKOS_CLIENT_ID" Model="$AYATI_MODEL" \
        ModelProvider="${AYATI_MODEL_PROVIDER:-openai}" ControllerCount="$1" ExecutorCount="$2" \
        WorkosSecretArn="$(secret_arn workos-api-key)" ModelSecretArn="$(secret_arn model-api-key)" \
        WorkerServerSecretArn="$(secret_arn worker-control-server)" \
        WorkerCaSecretArn="$(secret_arn worker-control-ca)"
}

# Runs migrations as the owner, then grants the runtime role; fails on any non-zero exit.
setup() {
    local cluster task subnets group codes
    cluster=$AYATI_ENV
    subnets=$(output "$(stack network)" AppSubnets)
    group=$(output "$(stack network)" ControllerGroup)
    say "Database setup"
    task=$(deploy_aws ecs run-task --cluster "$cluster" --launch-type FARGATE \
        --task-definition "$(output "$(stack services)" SetupTaskDefinition)" \
        --network-configuration "awsvpcConfiguration={subnets=[$subnets],securityGroups=[$group],assignPublicIp=DISABLED}" \
        --query 'tasks[0].taskArn' --output text)
    deploy_aws ecs wait tasks-stopped --cluster "$cluster" --tasks "$task"
    codes=$(deploy_aws ecs describe-tasks --cluster "$cluster" --tasks "$task" \
        --query 'tasks[0].containers[].[name,exitCode]' --output text)
    echo "$codes"
    if awk '$2 != "0" {bad=1} END {exit !bad}' <<<"$codes"; then
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

up() {
    require_session
    trap still_running ERR
    local started; started=$(date -u +%s)
    echo "$started" > "$state/started"
    deploy_stack "$(stack network)" network.yaml Environment="$AYATI_ENV" \
        ZoneA=ap-south-1a ZoneB=ap-south-1b
    deploy_stack "$(stack data)" data.yaml Environment="$AYATI_ENV"
    deploy_stack "$(stack roles)" roles.yaml Environment="$AYATI_ENV" --capabilities CAPABILITY_NAMED_IAM
    services 0 0
    setup
    services "${AYATI_CONTROLLERS:-1}" "${AYATI_EXECUTORS:-1}"
    wait_stable
    echo "Up in $(( ($(date -u +%s) - started) / 60 )) minutes: https://$AYATI_HOSTNAME"
}

# Controllers accept only the exact schema they were built for, so a new image
# stops the old services, migrates, then starts again (a short outage in test).
deploy() {
    require_session
    trap still_running ERR
    services 0 0
    setup
    services "${AYATI_CONTROLLERS:-1}" "${AYATI_EXECUTORS:-1}"
    wait_stable
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

down() {
    require_session
    logs || true
    empty_bucket "$AYATI_ENV-$AYATI_AWS_ACCOUNT-ap-south-1-files"
    for (( i=${#stacks[@]}-1; i>=0; i-- )); do
        say "Delete $(stack "${stacks[$i]}")"
        deploy_aws cloudformation delete-stack --stack-name "$(stack "${stacks[$i]}")"
        deploy_aws cloudformation wait stack-delete-complete --stack-name "$(stack "${stacks[$i]}")"
    done
    if [ -f "$state/started" ]; then
        echo "Window lasted $(( ($(date -u +%s) - $(cat "$state/started")) / 60 )) minutes."
        rm -f "$state/started"
    fi
    verify_clean
}

# Tags can lag deletion, so billable services are also listed directly.
verify_clean() {
    say "Temporary resources still present"
    local found=0 line
    while read -r line; do
        [ -n "$line" ] && { echo "$line"; found=1; }
    done < <(
        observe_aws cloudformation describe-stacks --query "Stacks[?starts_with(StackName,'$AYATI_ENV-')].StackName" --output text
        observe_aws rds describe-db-clusters --query "DBClusters[?starts_with(DBClusterIdentifier,'$AYATI_ENV')].DBClusterIdentifier" --output text
        observe_aws ec2 describe-nat-gateways --filter Name=state,Values=pending,available,deleting \
            --query 'NatGateways[].NatGatewayId' --output text
        observe_aws ec2 describe-addresses --query 'Addresses[].PublicIp' --output text
        observe_aws elbv2 describe-load-balancers --query 'LoadBalancers[].LoadBalancerName' --output text
        observe_aws ecs list-clusters --query 'clusterArns' --output text
        observe_aws s3api list-buckets --query "Buckets[?starts_with(Name,'$AYATI_ENV-')].Name" --output text
    )
    [ "$found" = 0 ] && echo "None. Only the permanent layer remains."
    return "$found"
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

command=${1:-}; shift || true
case "$command" in
    unlock) unlock "$@" ;;
    foundation) require_session; foundation "$@" ;;
    secret) require_session; secret "$@" ;;
    worker-tls) require_session; worker_tls ;;
    image) require_session; image ;;
    up) up ;;
    deploy) deploy ;;
    setup) require_session; setup ;;
    status) status ;;
    check) check ;;
    logs) logs "$@" ;;
    cost) cost "$@" ;;
    down) down ;;
    verify-clean) verify_clean ;;
    *) sed -n '2,16p' "$0"; exit 2 ;;
esac
