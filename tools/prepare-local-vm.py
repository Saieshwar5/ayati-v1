#!/usr/bin/env python3
"""Build one private local guest from a pinned Ubuntu image; no root daemon."""
import argparse
import io
import hashlib
import http.server
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import threading
import time
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / '.cache/vm-python'))
import pycdlib  # Pinned development dependency installed by install-local-vm.py.

IMAGE = ROOT / '.cache/vm-images/ubuntu-24.04-minimal-cloudimg-amd64.img'
IMAGE_SHA = '2f119f50bf29fe40e73b69ae2a736e8210d8146796f036e0324315b6b3a777df'


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def ssh_args(directory, port, agent_id):
    return ['ssh', '-T', '-F', '/dev/null', '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes',
            '-o', f'UserKnownHostsFile={directory / "known_hosts"}',
            '-o', 'GlobalKnownHostsFile=/dev/null', '-o', f'HostKeyAlias=ayati-{agent_id}',
            '-o', 'IdentitiesOnly=yes', '-o', 'ConnectTimeout=2', '-o', 'ConnectionAttempts=1',
            '-o', 'LogLevel=ERROR', '-i', str(directory / 'client_key'),
            '-p', str(port), 'ayati@127.0.0.1']


def prepare(directory, agent_id):
    require(re.fullmatch(r'[a-z0-9-]{1,48}', agent_id), 'Invalid agent identity')
    require(',' not in str(directory) and len(os.fsencode(directory / 'qmp.sock')) < 104,
            'VM directory must fit a private socket path and cannot contain commas')
    for name in ['ssh', 'ssh-keygen', 'socat']:
        require(shutil.which(name) is not None, f'Missing required local tool: {name}')
    require(Path('/usr/bin/socat').is_file(), 'This local proof expects /usr/bin/socat')
    directory.mkdir(parents=True, mode=0o700)
    require(not any(directory.iterdir()), 'Refusing to overwrite an existing VM directory')
    require(digest(IMAGE) == IMAGE_SHA, 'Run tools/install-local-vm.py for the pinned image')
    guest = ROOT / 'target/x86_64-unknown-linux-gnu/release/ayati-guest'
    goose = ROOT / '.cache/goose/v1.54.0/goose'
    for path in [guest, goose]:
        require(path.is_file(), f'Missing {path}; build/install the pinned guest components first')
    # Project-local QEMU is useful on Arch without a privileged package install.
    qemu_root = ROOT / '.cache/qemu'
    qemu = Path(os.environ.get('AYATI_QEMU_BIN', str(qemu_root / 'usr/bin/qemu-system-x86_64'))).resolve()
    qemu_img = qemu.with_name('qemu-img')
    firmware = Path(os.environ.get('AYATI_OVMF_DIR', str(qemu_root / 'usr/share/edk2/x64'))).resolve()
    library = str(qemu_root / 'usr/lib') if qemu.is_relative_to(qemu_root) else ''
    env = {**os.environ}
    if library:
        env['LD_LIBRARY_PATH'] = library
    subprocess.run([str(qemu), '--version'], env=env, check=True)
    for name in ['client_key', 'host_key']:
        subprocess.run(['ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-f', str(directory / name)], check=True)
    public = (directory / 'client_key.pub').read_text().strip()
    host_public = (directory / 'host_key.pub').read_text().strip()
    (directory / 'known_hosts').write_text(f'ayati-{agent_id} {host_public}\n')
    shutil.copyfile(firmware / 'OVMF_VARS.4m.fd', directory / 'vars.fd')
    subprocess.run([str(qemu_img), 'create', '-f', 'qcow2', '-F', 'qcow2', '-b', str(IMAGE), str(directory / 'disk.qcow2'), '8G'], env=env, check=True)
    binaries = {'goose': goose, 'ayati-guest': guest}
    hashes = {name: digest(path) for name, path in binaries.items()}
    seed = build_seed(agent_id, public, (directory / 'host_key').read_text(), host_public, hashes)

    class Bootstrap(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            if self.path.removeprefix('/') in binaries:
                body = binaries[self.path[1:]].read_bytes()
            else:
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *_):
            pass

    # Authentication material is supplied on a private read-only seed disk,
    # never through the temporary loopback artifact server.
    iso = pycdlib.PyCdlib()
    iso.new(interchange_level=3, rock_ridge='1.09', vol_ident='CIDATA')
    for name, contents in seed.items():
        data = contents.encode()
        iso.add_fp(io.BytesIO(data), len(data), iso_path='/' + name[1:].replace('-', '_').upper() + '.;1', rr_name=name[1:])
    iso.write(str(directory / 'seed.iso'))
    iso.close()
    (directory / 'seed.iso').chmod(0o600)

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Bootstrap)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    qmp = directory / 'qmp.sock'
    args = [str(qemu), '-machine', 'q35,accel=kvm,dump-guest-core=off,mem-merge=off', '-cpu', 'host',
            '-m', '768', '-smp', '1', '-nodefaults', '-no-user-config', '-display', 'none', '-monitor', 'none',
            '-serial', f'file:{directory / "console.log"}', '-qmp', f'unix:{qmp},server=on,wait=off',
            '-L', str(qemu_root / 'usr/share/qemu'),
            '-drive', f'if=pflash,format=raw,readonly=on,file={firmware / "OVMF_CODE.4m.fd"}',
            '-drive', f'if=pflash,format=raw,file={directory / "vars.fd"}',
            '-drive', f'file={directory / "disk.qcow2"},if=virtio,format=qcow2',
            '-drive', f'file={directory / "seed.iso"},if=virtio,format=raw,readonly=on',
            '-netdev', f'user,id=n1,restrict=on,ipv6=off,hostfwd=tcp:127.0.0.1:{port}-:22,guestfwd=tcp:10.0.2.100:80-cmd:/usr/bin/socat - TCP:127.0.0.1:{server.server_port}',
            '-device', 'virtio-net-pci,netdev=n1,romfile=']
    start = time.monotonic()
    with (directory / 'qemu.log').open('wb') as log:
        process = subprocess.Popen(args, env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
        try:
            ready = False
            while time.monotonic() - start < 180:
                if process.poll() is not None:
                    raise RuntimeError('QEMU exited; inspect the private qemu.log')
                result = subprocess.run(ssh_args(directory, port, agent_id), input=json.dumps({'version': 1, 'agent_id': agent_id, 'op': 'probe'}) + '\n', capture_output=True, text=True, timeout=5)
                if result.returncode == 0:
                    ready = json.loads(result.stdout)
                    break
                time.sleep(1)
            require(ready, 'Guest setup timed out; inspect private console.log and qemu.log')
            storage = subprocess.run(ssh_args(directory, port, agent_id), input=json.dumps({'version': 1, 'agent_id': agent_id, 'op': 'storage_probe'}) + '\n', capture_output=True, text=True, timeout=5)
            require(storage.returncode == 0 and json.loads(storage.stdout)['value'] == '', 'Fresh VM unexpectedly inherited workspace data')
            wrong_identity = subprocess.run(ssh_args(directory, port, agent_id), input=json.dumps({'version': 1, 'agent_id': 'not-this-agent', 'op': 'probe'}) + '\n', capture_output=True, text=True, timeout=5)
            require(wrong_identity.returncode != 0, 'Guest accepted a different agent identity')
            print(json.dumps({'guest': ready, 'fresh_workspace': True, 'rejects_other_identity': True, 'first_boot_seconds': round(time.monotonic() - start, 2)}), flush=True)
            with socket.socket(socket.AF_UNIX) as q:
                q.connect(str(qmp))
                q.recv(4096)
                q.sendall(b'{"execute":"qmp_capabilities"}\n')
                q.recv(4096)
                q.sendall(b'{"execute":"system_powerdown"}\n')
            try:
                require(process.wait(timeout=20) == 0, 'Guest VM exited unsuccessfully during shutdown')
            except subprocess.TimeoutExpired:
                raise RuntimeError('Guest did not shut down cleanly; refusing to declare image ready')
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            server.shutdown()
            server.server_close()
    (directory / 'seed.iso').unlink()
    (directory / 'host_key').unlink()  # Host only needs the pinned public identity after provisioning.
    (directory / 'host_key.pub').unlink()
    manifest = {'version': 1, 'agent_id': agent_id, 'qemu': str(qemu), 'qemu_img': str(qemu_img),
                'library_path': library, 'firmware_code': str(firmware / 'OVMF_CODE.4m.fd'),
                'firmware_data': str(qemu_root / 'usr/share/qemu'),
                'directory': str(directory), 'memory_mib': 768, 'vcpus': 1,
                'base_sha256': IMAGE_SHA, 'guest_sha256': hashes['ayati-guest'], 'goose_version': '1.54.0'}
    path = directory / 'vm.json'
    path.write_text(json.dumps(manifest, indent=2) + '\n')
    path.chmod(0o600)
    print(f'Ready VM manifest: {path}', flush=True)


def build_seed(agent_id, public, host_key, host_public, hashes):
    setup = f'''
import hashlib,pathlib,urllib.request,os,pwd,subprocess
for name,digest in {hashes!r}.items():
    blob=urllib.request.urlopen('http://10.0.2.100/'+name,timeout=30).read()
    if hashlib.sha256(blob).hexdigest()!=digest:raise RuntimeError('Guest artifact checksum mismatch')
    path=pathlib.Path('/opt/ayati/goose' if name=='goose' else '/usr/local/bin/ayati-guest')
    path.parent.mkdir(parents=True,exist_ok=True)
    path.write_bytes(blob);path.chmod(0o755)
u=pwd.getpwnam('ayati')
for name in ['','workspace','goose','goose/config']:
    path=pathlib.Path('/var/lib/ayati')/name
    path.mkdir(parents=True,exist_ok=True);path.chmod(0o700);os.chown(path,u.pw_uid,u.pw_gid)
pathlib.Path('/etc/cloud/cloud-init.disabled').touch()
subprocess.run(['systemctl','daemon-reexec'],check=True)
subprocess.run(['systemctl','mask','--now','snapd.service','snapd.socket','snapd.seeded.service','apt-daily.timer','apt-daily-upgrade.timer','unattended-upgrades.service'],check=False)
subprocess.run(['systemctl','restart','ssh'],check=True)
'''
    files = [{'path':'/etc/systemd/system.conf.d/ayati.conf','content':'[Manager]\nDefaultTimeoutStopSec=10s\n','permissions':'0644'}, {'path': '/etc/ayati/agent-id', 'content': agent_id + '\n', 'permissions': '0644'},
             {'path': '/etc/ssh/ssh_host_ed25519_key', 'content': host_key, 'permissions': '0600'},
             {'path': '/etc/ssh/ssh_host_ed25519_key.pub', 'content': host_public + '\n', 'permissions': '0644'},
             {'path': '/etc/ssh/sshd_config.d/00-ayati.conf', 'content': 'PasswordAuthentication no\nPermitRootLogin no\nAllowUsers ayati\nAllowTcpForwarding no\nAllowAgentForwarding no\nX11Forwarding no\nPermitTunnel no\nPermitUserEnvironment no\nUseDNS no\n', 'permissions': '0644'}]
    config = {'users': [{'name': 'ayati', 'shell': '/bin/sh', 'lock_passwd': True,
                        'ssh_authorized_keys': [f'restrict,command="/usr/local/bin/ayati-guest bridge" {public}']}],
              'ssh_pwauth': False, 'disable_root': True, 'ssh_deletekeys': False,
              'ssh_genkeytypes': [], 'write_files': files, 'runcmd': [['python3', '-c', setup]]}
    return {'/meta-data': json.dumps({'instance-id': f'ayati-{agent_id}', 'local-hostname': f'ayati-{agent_id}'}),
            '/user-data': '#cloud-config\n' + json.dumps(config), '/vendor-data': '{}'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', required=True)
    parser.add_argument('--agent-id', default='local-agent')
    args = parser.parse_args()
    directory = Path(args.directory).resolve()
    # Use a stable final directory component as the host-key alias.
    prepare(directory, args.agent_id)
