"""Owned container resources and private loopback DB access for image rehearsals."""
import json
import re
import secrets
import socket
import subprocess


def prepare(evidence, image):
    if not re.fullmatch(r'sha256:[a-f0-9]{64}', image):
        raise RuntimeError('Supply an immutable local image ID, not a mutable tag')
    result = subprocess.run(['docker', 'image', 'inspect', image], capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError('Cannot inspect the specified local image')
    info = json.loads(result.stdout)[0]
    if info['Id'] != image or info['Architecture'] != 'amd64' or info['Os'] != 'linux':
        raise RuntimeError('Expected the specified Linux amd64 image')
    if info['Config']['User'] != '10001:10001':
        raise RuntimeError('Release image must run as its restricted service user')
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        port = listener.getsockname()[1]
    owned = evidence / 'owned-containers.txt'
    owned.touch(mode=0o600)
    config = {'version': 1, 'image': image, 'session': secrets.token_hex(16),
              'port': port, 'owner_role': 'relay_image_owner',
              'owner_password': secrets.token_urlsafe(32), 'app_password': secrets.token_urlsafe(32),
              'containers_file': str(owned), 'audit_file': str(evidence / 'container-audit.jsonl')}
    path = evidence / 'image-config.json'
    path.write_text(json.dumps(config) + '\n')
    path.chmod(0o600)
    public = {'id': image, 'user': info['Config']['User'], 'architecture': info['Architecture'],
              'entrypoint': info['Config']['Entrypoint'], 'repo_digests': info.get('RepoDigests', []),
              'session': config['session'], 'network': 'host loopback, current local-only transport',
              'pid_namespace': 'private per container'}
    return config, path, public


def role_sql(config):
    # Generated URL-safe passwords are supplied over psql stdin, never argv/logs.
    return (f"ALTER ROLE relay_app PASSWORD '{config['app_password']}';\n"
            f"CREATE ROLE relay_image_owner LOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE PASSWORD '{config['owner_password']}';\n")


def stop_owned(config):
    from pathlib import Path
    for name in reversed(Path(config['containers_file']).read_text().splitlines()):
        if not re.fullmatch('ayati-image-' + re.escape(config['session']) + r'-[a-z]+-[a-f0-9]{32}', name):
            raise RuntimeError('Refusing a foreign container cleanup name')
        inspected = subprocess.run(['docker', 'container', 'inspect', name], capture_output=True, text=True)
        if inspected.returncode:
            # Absence is expected after an earlier cleanup; a daemon outage is not.
            if 'No such container' in inspected.stderr or 'No such object' in inspected.stderr:
                continue
            raise RuntimeError('Cannot verify an owned container for cleanup')
        info = json.loads(inspected.stdout)[0]
        if (info['Config'].get('Labels', {}).get('ayati.test.session') != config['session']
                or info['Image'] != config['image']):
            raise RuntimeError('Refusing a container with foreign ownership labels')
        removed = subprocess.run(['docker', 'rm', '--force', name], capture_output=True)
        if removed.returncode:
            raise RuntimeError('Owned container cleanup failed')
