#!/usr/bin/python3
# OFFLINE TEST ONLY: actual Git is delegated except fixed immutable diff failure injection.
import json
import os
from pathlib import Path
import sys
import time
cfg = json.loads(Path(CONFIG_PATH_LITERAL).read_text())
args = sys.argv[1:]
if 'diff' in args and '--literal-pathspecs' in args:
    assert args[:4] == ['--no-replace-objects', '--literal-pathspecs', '-c', 'core.fsmonitor=false'], args
    assert '--no-ext-diff' in args and '--no-textconv' in args and '--no-renames' in args
    assert os.environ['GIT_CONFIG_NOSYSTEM'] == '1'
    assert os.environ['GIT_CONFIG_GLOBAL'] == '/dev/null'
    assert os.environ['GIT_ATTR_NOSYSTEM'] == '1'
    assert os.environ['GIT_OPTIONAL_LOCKS'] == '0'
    assert 'GIT_INDEX_FILE' not in os.environ
    # Parent's immutable comparison passes only actual full fixed OIDs from task.
    offset = args.index('--')
    assert all(len(oid) in (40, 64) and all(ch in '0123456789abcdef' for ch in oid) for oid in args[offset-2:offset])
    if cfg['mode'] == 'failed':
        print('offline fixed diff failure', file=sys.stderr)
        sys.exit(7)
    if cfg['mode'] == 'truncated':
        sys.stdout.write('x' * 70000)
        sys.exit(0)
    if cfg['mode'] in ['timeout', 'cancel']:
        Path(cfg['marker']).write_text(str(os.getpid()))
        time.sleep(10)
        sys.exit(7)
os.execv('/usr/bin/git', ['/usr/bin/git'] + args)
