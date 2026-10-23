#!/usr/bin/env python3
"""Build a local immutable release image and retain isolated image/load checks."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def digest(path):
    with path.open('rb') as file:
        return hashlib.file_digest(file, 'sha256').hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', help='Reuse an existing immutable sha256 image ID')
    parser.add_argument('--build-jobs', type=int, choices=range(1, 5), default=2)
    parser.add_argument('--smoke-only', action='store_true', help='Run image/mixed/recovery checks without the two ordinary load matrices')
    args = parser.parse_args()
    if args.image and not re.fullmatch(r'sha256:[a-f0-9]{64}', args.image):
        parser.error('Use an immutable local image ID')
    (ROOT / '.tmp').mkdir(exist_ok=True)
    evidence = Path(tempfile.mkdtemp(prefix='release-image-', dir=ROOT / '.tmp'))
    os.chmod(evidence, 0o700)
    source = ['Dockerfile', '.dockerignore', 'Cargo.toml', 'Cargo.lock',
              'deploy/container-entrypoint.sh', 'tools/install-goose.sh',
              'tools/build-prepared-goose.py', 'tools/patches/goose-1.54.0-preparation.patch']
    report = {'version': 1, 'began_utc': datetime.now(timezone.utc).isoformat(),
              'base_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'dirty': bool(subprocess.check_output(['git', 'diff', 'HEAD', '--', 'Dockerfile', '.dockerignore', 'src', 'crates', 'web', 'deploy', 'tools'], cwd=ROOT)),
              'build_inputs': {name: digest(ROOT / name) for name in source},
              'build_origin': 'supplied image; checkout hashes describe test context only' if args.image else 'built from recorded checkout inputs',
              'runs': [], 'errors': [], 'passed': False}
    save = lambda: (evidence / 'verification.json').write_text(json.dumps(report, indent=2) + '\n')
    save()
    print('Retained image verification:', evidence, flush=True)
    try:
        image = args.image
        if not image:
            image_file = evidence / 'image-id.txt'
            command = ['docker', 'buildx', 'build', '--load', '--platform', 'linux/amd64',
                       '--progress', 'plain', '--build-arg', 'BUILD_JOBS=' + str(args.build_jobs),
                       '--iidfile', str(image_file), '--tag', 'ayati:verify-' + evidence.name.removeprefix('release-image-'), '.']
            with (evidence / 'build.log').open('w') as output:
                built = subprocess.run(command, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT)
            if built.returncode:
                raise RuntimeError('Image build failed; inspect build.log')
            image = image_file.read_text().strip()
        if not re.fullmatch(r'sha256:[a-f0-9]{64}', image):
            raise RuntimeError('Build did not return an immutable image ID')
        report['image'] = image
        save()
        suites = [('image', 200)] + ([] if args.smoke_only else [('matrix', 100), ('matrix', 200)])
        for suite, cpu in suites:
            log = evidence / f'{suite}-{cpu}.log'
            command = [sys.executable, 'tools/run-agent-load.py', '--suite', suite,
                       '--image', image, '--executor-cpu', str(cpu)]
            with log.open('w') as output:
                result = subprocess.run(command, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT)
            pointer = next((line.split(': ', 1)[1] for line in log.read_text().splitlines()
                            if line.startswith('Retained bounded rehearsal: ')), None)
            row = {'suite': suite, 'cpu_percent': cpu, 'exit_code': result.returncode, 'evidence': pointer}
            report['runs'].append(row)
            save()
            print(suite, cpu, 'exit', result.returncode, flush=True)
            if result.returncode or not pointer:
                raise RuntimeError('Container rehearsal failed; inspect retained run evidence')
            meta = json.loads((Path(pointer) / 'result.json').read_text())
            if meta['image']['id'] != image or meta['errors']:
                raise RuntimeError('Image identity or cleanup verification failed')
            text = (Path(pointer) / 'run.log').read_text()
            row['journeys_passed'] = int(re.search(r'ℹ tests (\d+)', text).group(1))
            if suite == 'image':
                mixed = [json.loads(line.removeprefix('MIXED_CASE ')) for line in text.splitlines() if line.startswith('MIXED_CASE ')]
                if len(mixed) != 4:
                    raise RuntimeError('Missing mixed workload timing coverage')
                row['ready_capacity_targets_met'] = all(all(case['provisional_targets'].values()) for case in mixed)
            if suite == 'matrix':
                accepted = next(line.removeprefix('LOAD_ACCEPTANCE ') for line in text.splitlines() if line.startswith('LOAD_ACCEPTANCE '))
                row['ready_capacity_targets_met'] = json.loads(accepted)['ready_capacity_targets_met']
            save()
        report['correctness_passed'] = True
        report['passed'] = all(row.get('ready_capacity_targets_met', True) for row in report['runs'])
        if not report['passed']:
            report['errors'].append('Correctness passed but a named ready-capacity latency target missed')
    except (Exception, KeyboardInterrupt) as error:
        report['errors'].append(str(error) or 'Interrupted')
    finally:
        report['finished_utc'] = datetime.now(timezone.utc).isoformat()
        save()
    print('Image verification:', 'passed' if report['passed'] else 'incomplete; inspect verification.json', flush=True)
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
