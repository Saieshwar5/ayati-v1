#!/usr/bin/env python3
"""Assemble pinned upstream browser artifacts for an offline guest install."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def install():
    pins = json.loads((ROOT / 'runtime/browser-release.json').read_text())
    pins['libraries'] = json.loads((ROOT / 'runtime' / pins['libraries_manifest']).read_text())
    cache = ROOT / '.cache/browser' / ('v' + pins['agent_browser']['version'])
    cache.mkdir(parents=True, exist_ok=True)
    artifacts = [(pins['agent_browser'], 'agent-browser'), (pins['chrome'], 'chrome.zip')]
    artifacts += [(item, item['name'] + '.deb') for item in pins['libraries']]
    paths = {}
    (cache / 'artifacts').mkdir(exist_ok=True)
    for item, name in artifacts:
        path = cache / 'artifacts' / (item['sha256'] + '-' + name)
        legacy = cache / name
        if not path.exists() and legacy.exists():
            with legacy.open('rb') as stream:
                if hashlib.file_digest(stream, 'sha256').hexdigest() == item['sha256']:
                    os.link(legacy, path)
        if not path.exists():
            temporary = path.with_suffix('.partial')
            urllib.request.urlretrieve(item['url'], temporary)
            temporary.rename(path)
        with path.open('rb') as stream:
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        if digest != item['sha256']:
            raise RuntimeError(f'Checksum mismatch: {name}')
        paths[name] = path
    stage = cache / 'bundle'
    if stage.exists():
        shutil.rmtree(stage)  # Only this installer's generated assembly tree.
    stage.mkdir(exist_ok=True)
    with zipfile.ZipFile(paths['chrome.zip']) as archive:
        archive.extractall(stage)
    chrome = stage / 'chrome-linux64'
    for name in ['chrome', 'chrome_crashpad_handler', 'chrome_sandbox']:
        (chrome / name).chmod(0o755)
    libraries = stage / 'libraries'
    libraries.mkdir(exist_ok=True)
    for item in pins['libraries']:
        subprocess.run(['bsdtar', '-xf', str(paths[item['name'] + '.deb']),
                        '--include', 'data.tar.*', '-C', str(cache)], check=True)
        for archive in cache.glob('data.tar.*'):
            subprocess.run(['bsdtar', '-xf', str(archive), '-C', str(libraries)], check=True)
            archive.unlink()
    # Debian packages contain absolute fontconfig links. Relocate them inside
    # the bundle so Python's safe extraction never follows a host/guest path.
    for path in libraries.rglob('*'):
        if path.is_symlink() and os.readlink(path).startswith('/'):
            target = libraries / os.readlink(path).lstrip('/')
            path.unlink()
            path.symlink_to(os.path.relpath(target, path.parent))
    shutil.copy2(paths['agent-browser'], stage / 'agent-browser')
    (stage / 'agent-browser').chmod(0o755)
    (stage / 'chrome').write_text('#!/bin/sh\n'
        'export LD_LIBRARY_PATH=/opt/ayati/browser/libraries/usr/lib/x86_64-linux-gnu\n'
        'export FONTCONFIG_FILE=/opt/ayati/browser/fonts.conf\n'
        'exec /opt/ayati/browser/chrome-linux64/chrome "$@"\n')
    (stage / 'chrome').chmod(0o755)
    (stage / 'fonts.conf').write_text('<?xml version="1.0"?><!DOCTYPE fontconfig SYSTEM "fonts.dtd"><fontconfig>'
        '<dir>/opt/ayati/browser/libraries/usr/share/fonts</dir>'
        '<cachedir>/var/lib/ayati-browser/fontcache</cachedir></fontconfig>')
    shutil.copy2(ROOT / 'runtime/browser-release.json', stage / 'release.json')
    shutil.copy2(ROOT / 'runtime/agent-browser-LICENSE.txt', stage / 'agent-browser-LICENSE.txt')
    shutil.copy2(ROOT / 'runtime/browser-libraries.json', stage / 'browser-libraries.json')
    output = cache / 'browser.tar.gz'
    with tarfile.open(output, 'w:gz') as archive:
        for path in sorted(stage.iterdir()):
            archive.add(path, arcname=path.name)
    print(f'Guest browser bundle: {output}')


if __name__ == '__main__':
    install()
