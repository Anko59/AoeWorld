#!/usr/bin/python3
# OFFLINE TEST SHIM ONLY. Fixed endpoint contracts are asserted before synthetic
# responses; no model-accessible override or production trust flag is introduced.
import json
import os
from pathlib import Path
import subprocess
import sys
import time

CONFIG = Path(CONFIG_PATH_LITERAL)
cfg = json.loads(CONFIG.read_text())
tool = Path(sys.argv[0]).name
args = sys.argv[1:]
mode = cfg['mode']
record = {'tool': tool, 'args': args, 'network': False, 'fetch': False}

def append():
    with Path(cfg['transcript']).open('a') as stream:
        stream.write(json.dumps(record) + '\n')

if tool == 'gh':
    assert args[:5] == ['api', '--hostname', 'github.com', '--method', 'GET'], args
    assert len(args) == 6
    endpoint = args[-1]
    assert endpoint in ['repos/Example/Policy', 'repos/Example/Policy/branches/dev', 'repos/Example/Policy/branches/dev/protection'], endpoint
    append()
    if mode == 'api-exit':
        print('fixture API denied', file=sys.stderr)
        sys.exit(7)
    if mode == 'api-truncated':
        print('x' * 70000)
        sys.exit(0)
    if mode == 'api-json':
        print('not-json')
        sys.exit(0)
    if endpoint == 'repos/Example/Policy':
        response = {'id': 8 if mode == 'repository-mismatch' else 7, 'full_name': 'Example/Policy'}
    elif endpoint.endswith('/protection'):
        response = {'required_status_checks': {'strict': True, 'contexts': [], 'checks': [{'context': 'required', 'app_id': 17}]}}
        if mode == 'checks-malformed':
            response['required_status_checks']['checks'] = [{'context': 17}]
    else:
        if mode == 'cancel':
            Path(cfg['started']).write_text(str(os.getpid()))
            print('fixture waiting for cancellation', flush=True)
            time.sleep(10)
        response = {'name': 'dev', 'protected': True, 'commit': {'sha': cfg['oid']}}
    print(json.dumps(response))
    sys.exit(0)

if tool == 'git':
    network = 'ls-remote' in args or 'fetch' in args
    record['network'] = network
    if network:
        # Measure actual inherited transport environment, not just the argv builder.
        for forbidden in ['GH_TOKEN', 'GITHUB_TOKEN', 'GIT_INDEX_FILE', 'GIT_DIR', 'GIT_WORK_TREE', 'GIT_CONFIG_COUNT']:
            assert forbidden not in os.environ, forbidden
        assert os.environ['GIT_CONFIG_GLOBAL'] == '/dev/null'
        assert os.environ['GIT_CONFIG_SYSTEM'] == '/dev/null'
        assert os.environ['GIT_NO_REPLACE_OBJECTS'] == '1'
        assert Path(os.environ['HOME']).name == 'home'
        assert Path(os.environ['HOME']).parent.name.startswith('policy-fetch-')
        for restriction in ['core.hooksPath=/dev/null', 'credential.helper=', 'protocol.file.allow=never', 'protocol.ext.allow=never', 'http.followRedirects=false', 'fetch.fsckObjects=true']:
            assert restriction in args, restriction
        record['credentialless'] = True
    if 'ls-remote' in args:
        assert args[-4:] == ['ls-remote', '--refs', 'https://github.com/Example/Policy.git', 'refs/heads/dev'], args
        # There are four command arguments, independent of preceding -c flags.
        append()
        oid = 'd' * 40 if mode == 'moving' else cfg['oid']
        print(oid + '\trefs/heads/dev')
        sys.exit(0)
    if 'fetch' in args:
        record['fetch'] = True
        expected = ['fetch', '--no-tags', '--no-recurse-submodules', '--no-write-fetch-head', '--depth=1', 'https://github.com/Example/Policy.git', cfg['oid']]
        assert args[-7:] == expected, args
        append()
        # ONLY test transport replacement: production argv is checked first;
        # trusted Git performs local raw-object import instead of any network.
        offset = args.index('fetch')
        args = args[:offset] + ['-c', 'protocol.file.allow=always'] + args[offset:]
        args[-2] = cfg['bundle']
        # Bundle protocol does not advertise shallow/full-OID wire negotiation.
        # Production argv was already asserted exactly above; adapt ONLY fixture
        # transport while later cat-file/tree/snapshot checks bind the actual OID.
        args.remove('--depth=1')
        args[-1] = 'HEAD'
        os.execv('/usr/bin/git', ['/usr/bin/git'] + args)
    private = str(Path.cwd()).startswith(cfg['evidence'] + '/')
    append()
    if private and 'cat-file' in args and '-t' in args and mode == 'object-type':
        print('blob')
        sys.exit(0)
    if private and 'rev-parse' in args and '--verify' in args and args[-1] == cfg['oid'] + '^{tree}' and mode == 'bad-tree':
        print('not-an-object-id')
        sys.exit(0)
    os.execv('/usr/bin/git', ['/usr/bin/git'] + args)

if tool == 'docker':
    assert args == ['image', 'inspect', '--format', '{{.Id}}', cfg['reference']], args
    append()
    if mode == 'image-exit':
        print('fixture image unavailable', file=sys.stderr)
        sys.exit(9)
    if mode == 'image-utf8':
        sys.stdout.buffer.write(b'\xff\n')
        sys.exit(0)
    anchor = Path(cfg['anchor'])
    # Paths are fixed inside the owning test TempDir, never model input.
    assert anchor.parent == CONFIG.parent
    if mode == 'anchor-bytes':
        anchor.write_text(anchor.read_text() + '\n')
    if mode == 'anchor-symlink':
        copy = Path(cfg['anchor_copy'])
        assert copy.parent == CONFIG.parent
        assert anchor.resolve() == anchor
        anchor.unlink()
        anchor.symlink_to(copy)
    if mode == 'policy-shadow':
        matches = list(Path(cfg['evidence']).rglob('gates/judge.json'))
        assert len(matches) == 1, matches
        root = matches[0].parent.parent
        assert root.is_relative_to(Path(cfg['evidence']))
        (root / 'shadow.txt').write_text('extra untracked policy input\n')
    print('sha256:' + '3' * 64 if mode == 'image-id' else cfg['actual_id'])
    sys.exit(0)

raise AssertionError('unexpected shim executable: ' + tool)
