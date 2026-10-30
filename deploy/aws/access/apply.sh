#!/usr/bin/env bash
# Creates or updates the two access roles and the runtime boundary (run once,
# and again after a policy file changes). Prints the plan unless --apply is given.
#
#   apply.sh [--apply]            boundary, observer role, deployer role (MFA)
#   apply.sh mfa-create           virtual MFA device for the CLI user (QR code file)
#   apply.sh mfa-enable C1 C2     two consecutive codes from the authenticator app
#   apply.sh lock-cli-user        CLI user may only assume the access roles
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
user=${AYATI_AWS_CLI_USER:-cuckoo-cli}
group=${AYATI_AWS_CLI_GROUP:-cuckoo-cli-group}
account=$(aws sts get-caller-identity --query Account --output text)
user_arn="arn:aws:iam::$account:user/$user"
mfa_arn="arn:aws:iam::$account:mfa/$user"
apply=false

render() { sed "s/\${ACCOUNT}/$account/g" "$here/$1"; }

run() {
    if $apply; then "$@"; else printf 'would run: %s\n' "$*" | cut -c1-160; fi
}

trust() {  # trust() <require-mfa: true|false>
    local condition=''
    [ "$1" = true ] && condition=',"Condition":{"Bool":{"aws:MultiFactorAuthPresent":"true"}}'
    printf '{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Principal":{"AWS":"%s"},"Action":"sts:AssumeRole"%s}]}' \
        "$user_arn" "$condition"
}

# A managed policy keeps at most five versions; drop the oldest non-default one.
put_policy() {  # put_policy <name> <file>
    local arn="arn:aws:iam::$account:policy/$1" document
    document=$(render "$2")
    if ! aws iam get-policy --policy-arn "$arn" >/dev/null 2>&1; then
        run aws iam create-policy --policy-name "$1" --policy-document "$document" \
            --tags Key=ayati:layer,Value=access
        return
    fi
    local versions
    versions=$(aws iam list-policy-versions --policy-arn "$arn" \
        --query 'Versions[?!IsDefaultVersion].VersionId' --output text)
    if [ "$(wc -w <<<"$versions")" -ge 4 ]; then
        run aws iam delete-policy-version --policy-arn "$arn" --version-id "$(tr '\t' '\n' <<<"$versions" | tail -1)"
    fi
    run aws iam create-policy-version --policy-arn "$arn" --policy-document "$document" --set-as-default
}

put_role() {  # put_role <name> <require-mfa> <max-seconds>
    if aws iam get-role --role-name "$1" >/dev/null 2>&1; then
        run aws iam update-assume-role-policy --role-name "$1" --policy-document "$(trust "$2")"
        run aws iam update-role --role-name "$1" --max-session-duration "$3"
    else
        run aws iam create-role --role-name "$1" --assume-role-policy-document "$(trust "$2")" \
            --max-session-duration "$3" --tags Key=ayati:layer,Value=access
    fi
}

access() {
    put_policy ayati-runtime-boundary runtime-boundary.json
    put_policy ayati-access-deployer-services deployer-services.json
    put_policy ayati-access-deployer-iam deployer-iam.json

    put_role ayati-access-observer false 43200
    run aws iam attach-role-policy --role-name ayati-access-observer \
        --policy-arn arn:aws:iam::aws:policy/ReadOnlyAccess
    run aws iam put-role-policy --role-name ayati-access-observer \
        --policy-name costs-and-logs --policy-document "$(render observer-extra.json)"

    put_role ayati-access-deployer true 28800
    for name in ayati-access-deployer-services ayati-access-deployer-iam; do
        run aws iam attach-role-policy --role-name ayati-access-deployer \
            --policy-arn "arn:aws:iam::$account:policy/$name"
    done
    cat <<EOF

Add to ~/.aws/config (the observer needs no MFA; env.sh unlock creates the deployer session):

[profile ayati-observer]
role_arn = arn:aws:iam::$account:role/ayati-access-observer
source_profile = default
region = ap-south-1
EOF
}

lock_cli_user() {
    local policy
    policy=$(printf '{"Version":"2012-10-17","Statement":[
{"Sid":"AssumeAccessRoles","Effect":"Allow","Action":"sts:AssumeRole","Resource":"arn:aws:iam::%s:role/ayati-access-*"},
{"Sid":"ManageOwnMfaAndKeys","Effect":"Allow","Action":["iam:CreateVirtualMFADevice","iam:EnableMFADevice","iam:ResyncMFADevice","iam:ListMFADevices","iam:GetUser","iam:ListAccessKeys","iam:CreateAccessKey","iam:UpdateAccessKey","iam:DeleteAccessKey"],"Resource":["%s","arn:aws:iam::%s:mfa/%s"]}]}' \
        "$account" "$user_arn" "$account" "$user")
    run aws iam put-group-policy --group-name "$group" --policy-name assume-ayati-access --policy-document "$policy"
    for managed in IAMFullAccess AmazonEC2FullAccess AmazonRDSFullAccess AmazonS3FullAccess; do
        run aws iam detach-group-policy --group-name "$group" --policy-arn "arn:aws:iam::aws:policy/$managed"
    done
    echo "After this, recover broader access only through the root account console."
}

case "${1:-}" in
    --apply) apply=true; access ;;
    "") access; echo; echo "Plan only. Re-run with --apply to make these changes." ;;
    mfa-create)
        out="${2:-$HOME/ayati-cli-mfa.png}"
        aws iam create-virtual-mfa-device --virtual-mfa-device-name "$user" \
            --bootstrap-method QRCodePNG --outfile "$out" --query VirtualMFADevice.SerialNumber --output text
        echo "Scan $out with an authenticator app, delete the file, then run: $0 mfa-enable CODE1 CODE2" ;;
    mfa-enable)
        aws iam enable-mfa-device --user-name "$user" --serial-number "$mfa_arn" \
            --authentication-code1 "$2" --authentication-code2 "$3"
        echo "MFA enabled for $user ($mfa_arn)." ;;
    lock-cli-user)
        [ "${2:-}" = --apply ] && apply=true
        lock_cli_user ;;
    *) sed -n '2,9p' "$0"; exit 2 ;;
esac
