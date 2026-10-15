#!/usr/bin/env python3
"""Download pinned VM development artifacts to .cache; no system installation."""
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def download(url, path, expected):
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists():
        temporary = path.with_suffix(path.suffix + '.part')
        print(f'Downloading {path.name}', flush=True)
        urllib.request.urlretrieve(url, temporary)
        temporary.rename(path)
    with path.open('rb') as stream:
        actual = hashlib.file_digest(stream, 'sha256').hexdigest()
    if actual != expected:
        raise RuntimeError(f'Checksum mismatch: {path}; preserve it for inspection and retry in a fresh cache')


def main():
    if platform.system() != 'Linux' or platform.machine() != 'x86_64':
        raise RuntimeError('This local VM proof supports Linux x86_64')
    pins = json.loads((ROOT / 'runtime/local-vm-release.json').read_text())
    # These are Arch packages, extracted inside the project, never installed into
    # /usr. Native package setup on other distributions is not automated here.
    release = dict(line.split('=', 1) for line in Path('/etc/os-release').read_text().splitlines() if '=' in line)
    if 'arch' not in (release.get('ID', '') + ' ' + release.get('ID_LIKE', '')).replace('\"', '').split():
        raise RuntimeError('This bundled helper supports Arch-family Linux; native QEMU/OVMF setup on other distributions is not yet validated')
    target = ROOT / '.cache/qemu'
    for package in pins['packages']:
        path = target / package['url'].rsplit('/', 1)[1]
        download(package['url'], path, package['sha256'])
        subprocess.run(['bsdtar', '-xf', str(path), '-C', str(target)], check=True)
    env = {**os.environ, 'LD_LIBRARY_PATH': str(target / 'usr/lib')}
    subprocess.run([str(target / 'usr/bin/qemu-system-x86_64'), '--version'], env=env, check=True)
    wheel = pins['seed_builder']
    wheel_path = ROOT / '.cache/vm-python/pycdlib.whl'
    download(wheel['url'], wheel_path, wheel['sha256'])
    with zipfile.ZipFile(wheel_path) as archive:
        archive.extractall(wheel_path.parent)
    image = pins['ubuntu']
    download(image['url'], ROOT / '.cache/vm-images' / image['url'].rsplit('/', 1)[1], image['sha256'])
    print('Pinned QEMU packages and Ubuntu image verified. No host services were changed.')


if __name__ == '__main__':
    main()
