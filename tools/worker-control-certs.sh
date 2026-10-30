#!/usr/bin/env bash
# Create a private certificate authority and a worker control server certificate
# for executors that reach controllers over a private network.
#   tools/worker-control-certs.sh <empty-directory> <host-or-ip>...
# Writes ca.pem (give to executors), server.pem and server.key (controllers only,
# 0600). The CA private key is deleted: issue a new set to rotate.
set -euo pipefail
[[ $# -ge 2 ]] || { echo 'Usage: worker-control-certs.sh <empty-directory> <host-or-ip>...'; exit 2; }
out=$1; shift
mkdir -p "$out"
[[ -z "$(ls -A "$out")" ]] || { echo 'Refusing a non-empty directory'; exit 1; }
umask 077
names=()
for name in "$@"; do
    if [[ "$name" =~ ^[0-9.]+$ || "$name" == *:* ]]; then names+=("IP:$name")
    elif [[ "$name" =~ ^[A-Za-z0-9.-]+$ ]]; then names+=("DNS:$name")
    else echo "Unsupported host name"; exit 1
    fi
done
san=$(IFS=,; echo "${names[*]}")
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes -days 825 \
    -subj '/CN=Ayati worker control CA' -keyout "$work/ca.key" -out "$out/ca.pem" \
    -addext 'basicConstraints=critical,CA:TRUE' -addext 'keyUsage=critical,keyCertSign' 2>/dev/null
openssl req -newkey ec -pkeyopt ec_paramgen_curve:P-256 -nodes \
    -subj '/CN=Ayati worker control' -keyout "$out/server.key" -out "$work/server.csr" 2>/dev/null
openssl x509 -req -in "$work/server.csr" -CA "$out/ca.pem" -CAkey "$work/ca.key" \
    -CAcreateserial -CAserial "$work/ca.srl" -days 825 -out "$out/server.pem" \
    -extfile <(printf 'subjectAltName=%s\nbasicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n' "$san") 2>/dev/null
chmod 0644 "$out/ca.pem" "$out/server.pem"
chmod 0600 "$out/server.key"
echo "Wrote $out/ca.pem, $out/server.pem and $out/server.key"
