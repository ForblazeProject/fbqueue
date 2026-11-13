#!/usr/bin/env python3
"""Build a source-only Windows test kit, verifying portable checks first."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import zipfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--pwsh', required=True, help='PowerShell for portable verification')
    parser.add_argument('--output', required=True, type=Path)
    options = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    kit = repo / 'tests/windows'
    env = dict(os.environ, FBQ_BUNDLE_TESTDIR=str(kit))
    parse_command = r'''$ErrorActionPreference='Stop'; $all=@(Get-ChildItem -LiteralPath $env:FBQ_BUNDLE_TESTDIR -Filter '*.ps1'); $all+=Get-Item (Join-Path $env:FBQ_BUNDLE_TESTDIR '../run_tests_core.ps1'); foreach($f in $all){$tokens=$null;$errors=$null;[Management.Automation.Language.Parser]::ParseFile($f.FullName,[ref]$tokens,[ref]$errors)|Out-Null;if($errors){throw ($errors|Out-String)}}; 'PASS: all PowerShell scripts parsed' '''
    checks = []
    for command in ([options.pwsh, '-NoProfile', '-Command', parse_command],
                    [options.pwsh, '-NoProfile', '-File', str(kit / 'Test-Harness.ps1')]):
        result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=45)
        checks.append(result.stdout + result.stderr)
        print(checks[-1].strip())
        if result.returncode:
            raise SystemExit(f'Portable validation failed: exit {result.returncode}')
    main_text = (kit / 'Run-Tests.ps1').read_text(encoding='utf-8-sig')
    worker_text = (kit / 'Case-Worker.ps1').read_text(encoding='utf-8-sig')
    catalog_match = re.search(r'\$catalog=@\(([^\n]+)\)', main_text)
    if catalog_match is None:
        raise SystemExit('Case catalog is missing')
    catalog = re.findall(r"'([^']+)'", catalog_match.group(1))
    implemented = re.findall(r"^        '([^']+)' \{", worker_text, re.M)
    if len(catalog) != len(set(catalog)) or set(catalog) != set(implemented):
        raise SystemExit('Case catalog/implementation mismatch')
    payload = {}
    for path in sorted(kit.iterdir()):
        if path.is_file() and path.suffix in {'.ps1', '.cs', '.cmd', '.md'}:
            payload[path.name] = path.read_bytes()
    payload['WINDOWS_SUPPORT.md'] = (repo / 'docs/WINDOWS_SUPPORT.md').read_bytes()
    payload['WINDOWS_VERIFICATION.md'] = (repo / 'docs/WINDOWS_VERIFICATION.md').read_bytes()
    sources = [repo / name for name in ('Cargo.toml', 'Cargo.lock', 'LICENSE', 'README.md', 'MANUAL.md', 'PBS_COMPATIBILITY.md')]
    sources += sorted((repo / 'src').glob('*.rs'))
    sources += [repo / 'docs' / name for name in ('DAEMON_RECOVERY_DESIGN.md', 'WINDOWS_SUPPORT.md', 'WINDOWS_VERIFICATION.md')]
    sources += sorted(path for path in (repo / 'tests').rglob('*')
                      if path.is_file() and path.suffix in {'.py', '.ps1', '.cs', '.cmd', '.sh', '.md'}
                      and '__pycache__' not in path.parts)
    sources += [repo / 'examples/windows' / name for name in ('test_job.bat', 'test_job.ps1')]
    for path in sources:
        relative = path.relative_to(repo).as_posix()
        if any(part in {'.git', '.env', 'AGENTS.md', 'node_modules', 'target'} for part in path.relative_to(repo).parts):
            raise SystemExit('Disallowed source path: ' + relative)
        payload['source/' + relative] = path.read_bytes()
    head = subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip()
    manifest = {
        'schema': 'fbqueue.windows.bundle.v1',
        'generated_at_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'git_head': head,
        'source_note': 'Includes local uncommitted changes; per-file hashes, not Git HEAD alone, identify this snapshot.',
        'native_windows_verified': False,
        'case_count': len(catalog),
        'cases': catalog,
        'files': [{'path': name, 'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data)}
                  for name, data in sorted(payload.items())],
    }
    payload['SOURCE-MANIFEST.json'] = (json.dumps(manifest, ensure_ascii=False, indent=2) + '\n').encode()
    payload['PORTABLE-VALIDATION.txt'] = (
        'Actual portable validation output; these are NOT Windows product results.\n\n' + '\n'.join(checks)
    ).encode()
    options.output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(options.output, 'x', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data in sorted(payload.items()):
            archive.writestr('fbqueue-windows-test-kit/' + name, data)
    with zipfile.ZipFile(options.output) as archive:
        if archive.testzip() is not None:
            raise SystemExit('Archive CRC verification failed')
        if len(archive.namelist()) != len(set(archive.namelist())):
            raise SystemExit('Duplicate ZIP entries')
        for entry in manifest['files']:
            actual = archive.read('fbqueue-windows-test-kit/' + entry['path'])
            if hashlib.sha256(actual).hexdigest() != entry['sha256']:
                raise SystemExit('Archive read-back hash mismatch: ' + entry['path'])
    print(json.dumps({'artifact': str(options.output), 'bytes': options.output.stat().st_size,
                      'archive_entries': len(payload), 'manifest_files': len(manifest['files']),
                      'cases': len(catalog), 'native_windows_verified': False,
                      'sha256': hashlib.sha256(options.output.read_bytes()).hexdigest()}, indent=2))


if __name__ == '__main__':
    main()
