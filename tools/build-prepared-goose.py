#!/usr/bin/env python3
"""Build the separately identified, pinned Goose preparation variant."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parent.parent
COMMIT = 'cd643d2126fffab4c6a6186905211ba868d6a657'
VERSION = '1.54.0+ayati.1'
FEATURES = 'rustls-tls'
SOURCE_URL = 'https://github.com/aaif-goose/goose.git'


def digest(path):
    with path.open('rb') as file:
        return hashlib.file_digest(file, 'sha256').hexdigest()


def tree_digest(directory):
    return {str(path.relative_to(directory)): ('link:' + os.readlink(path)
        if path.is_symlink() else digest(path))
        for path in directory.rglob('*') if path.is_file() or path.is_symlink()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-repo', type=Path, help='Optional local Goose Git repository containing the pinned commit')
    parser.add_argument('--jobs', type=int, choices=range(1, 9), default=4)
    parser.add_argument('--rebuild', action='store_true')
    args = parser.parse_args()
    patch = ROOT / 'tools/patches/goose-1.54.0-preparation.patch'
    install = ROOT / '.cache/goose/v1.54.0-ayati.1'
    manifest = install / 'manifest.json'
    binary = install / 'goose'
    if not args.rebuild and binary.is_file() and manifest.is_file():
        saved = json.loads(manifest.read_text())
        if saved['source_commit'] == COMMIT and saved['patch_sha256'] == digest(patch) and saved['binary_sha256'] == digest(binary):
            print('Verified prepared Goose already installed:', binary)
            return
        raise RuntimeError('Existing prepared artifact differs; inspect it or explicitly use --rebuild')
    cache = ROOT / '.cache/goose-prepared-build'
    cache.mkdir(parents=True, exist_ok=True)
    repository = args.source_repo.resolve() if args.source_repo else cache / 'repository'
    if not args.source_repo:
        if not (repository / 'HEAD').exists():
            subprocess.run(['git', 'init', '--bare', str(repository)], check=True)
        subprocess.run(['git', '-C', str(repository), 'fetch', '--depth=1', SOURCE_URL, COMMIT], check=True)
    actual = subprocess.check_output(['git', '-C', str(repository), 'rev-parse', COMMIT + '^{commit}'], text=True).strip()
    if actual != COMMIT:
        raise RuntimeError('Unexpected Goose source commit')
    source = cache / 'source'
    # Reuse only this exact already-patched snapshot, checked against its archive.
    with tempfile.TemporaryDirectory(prefix='goose-source-', dir=cache) as temporary:
        clean = Path(temporary)
        archive = cache / 'source.tar'
        with archive.open('wb') as file:
            subprocess.run(['git', '-C', str(repository), 'archive', COMMIT], stdout=file, check=True)
        with tarfile.open(archive) as tar:
            tar.extractall(clean, filter='data')
        subprocess.run(['git', 'apply', '--check', str(patch)], cwd=clean, check=True)
        subprocess.run(['git', 'apply', str(patch)], cwd=clean, check=True)
        if source.exists():
            if tree_digest(source) != tree_digest(clean):
                raise RuntimeError('Cached source has changes; move it aside to preserve it before rebuilding')
        else:
            shutil.copytree(clean, source, symlinks=True)
    target = cache / 'target'
    command = ['cargo', 'build', '--manifest-path', str(source / 'Cargo.toml'),
               '--target-dir', str(target), '--release', '--locked', '-p', 'goose-cli',
               '--bin', 'goose', '--no-default-features', '--features', FEATURES, '-j', str(args.jobs)]
    subprocess.run(command, cwd=ROOT, check=True)
    built = target / 'release/goose'
    output = subprocess.check_output([str(built), '--version'], text=True).strip()
    if output != VERSION:
        raise RuntimeError('Prepared Goose version mismatch')
    install.mkdir(parents=True, exist_ok=True)
    temporary_binary = install / 'goose.new'
    shutil.copy2(built, temporary_binary)
    temporary_binary.chmod(0o755)
    temporary_binary.replace(binary)
    for name in ['LICENSE', 'NOTICE']:
        if (source / name).is_file():
            shutil.copy2(source / name, install / name)
    saved = {'version': VERSION, 'preparation_contract': 1, 'source_url': SOURCE_URL,
             'source_commit': COMMIT, 'patch_sha256': digest(patch),
             'cargo_lock_sha256': digest(source / 'Cargo.lock'), 'binary_sha256': digest(binary),
             'features': FEATURES.split(','), 'profile': 'release',
             'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
             'modification': 'Opt-in no-inference token-counter initialization before session/new acknowledgment; distinct CLI version.'}
    manifest.write_text(json.dumps(saved, indent=2) + '\n')
    print('Installed prepared Goose:', binary)


if __name__ == '__main__':
    main()
