#!/usr/bin/env python3
"""Exercise installed Codex isolation without credentials, user history or inference.

Uses an expendable CODEX_HOME only for this test. The application itself preserves
its authenticated Codex home. Emits a bounded, non-sensitive JSON result.
"""
import argparse
import json
import os
import pathlib
import select
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--codex', default='codex')
    args = parser.parse_args()
    checks = {}
    with tempfile.TemporaryDirectory(prefix='live-coach-codex-isolation-') as folder:
        root = pathlib.Path(folder).resolve()
        home, project = root / 'codex', root / 'project'
        home.mkdir(); project.mkdir()
        inside, outside, marker = project / 'inside.txt', root / 'outside.txt', root / 'mcp-started'
        inside.write_text('public test fixture')
        outside.write_text('private test fixture')
        profile = 'live_coach_test'
        profile_config = {'filesystem': {':minimal': 'read', str(project): 'write'}, 'network': {'enabled': False}}
        inline = '{filesystem={":minimal"="read",' + json.dumps(str(project)) + '="write"},network={enabled=false}}'
        (home / 'config.toml').write_text(
            'sandbox_mode="danger-full-access"\n'
            '[mcp_servers.marker]\ncommand="/usr/bin/touch"\nargs=[' + json.dumps(str(marker)) + ']\n'
            '[features]\napps=true\nplugins=true\nhooks=true\n'
        )
        env = dict(os.environ, CODEX_HOME=str(home))
        # This is a real sandboxed subprocess, not an assertion about prompt text.
        for name, target, success in [('inside_read', inside, True), ('outside_read_denied', outside, False), ('codex_home_read_denied', home / 'config.toml', False)]:
            proc = subprocess.run([args.codex, '-c', f'permissions.{profile}={inline}', 'sandbox', '-P', profile, '-C', str(project), '--', '/bin/cat', str(target)], env=env, capture_output=True, timeout=20)
            checks[name] = (proc.returncode == 0) == success
            if not success:
                checks[name] = checks[name] and b'Operation not permitted' in proc.stderr
        flags = {
            'features.apps': 'false', 'features.hooks': 'false', 'features.plugins': 'false',
            'features.remote_plugin': 'false', 'mcp_servers.marker.enabled': 'false',
            'notify': '[]', 'web_search': '"disabled"', 'project_doc_max_bytes': '0',
            'approval_policy': '"on-request"', 'approvals_reviewer': '"user"',
            f'permissions.{profile}': inline, 'default_permissions': json.dumps(profile),
        }
        command = [args.codex]
        for key, value in flags.items(): command.extend(['-c', f'{key}={value}'])
        command.extend(['app-server', '--listen', 'stdio://'])
        proc = subprocess.Popen(command, cwd=project, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True, bufsize=1)
        def rpc(identifier, method, params):
            proc.stdin.write(json.dumps({'id': identifier, 'method': method, 'params': params}) + '\n'); proc.stdin.flush()
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                if select.select([proc.stdout], [], [], .25)[0]:
                    line = proc.stdout.readline()
                    if not line: raise RuntimeError('Codex exited before response')
                    data = json.loads(line)
                    if data.get('id') == identifier:
                        if 'error' in data: raise RuntimeError('Codex rejected isolation request')
                        return data['result']
            raise RuntimeError('Codex isolation request timed out')
        try:
            rpc(1, 'initialize', {'clientInfo': {'name': 'live_coach_isolation_test', 'version': '1'}, 'capabilities': {'experimentalApi': False}})
            proc.stdin.write('{"method":"initialized"}\n'); proc.stdin.flush()
            config = rpc(2, 'config/read', {'includeLayers': True})['config']
            checks['effective_mcp_disabled'] = config['mcp_servers']['marker']['enabled'] is False
            checks['effective_features_disabled'] = all(config['features'][key] is False for key in ('apps', 'hooks', 'plugins', 'remote_plugin'))
            thread = rpc(3, 'thread/start', {'cwd': str(project), 'ephemeral': True, 'approvalPolicy': 'on-request', 'approvalsReviewer': 'user'})
            checks['named_profile_acknowledged'] = thread['activePermissionProfile']['id'] == profile
            checks['network_disabled'] = thread['sandbox']['networkAccess'] is False
            checks['global_danger_full_access_overridden'] = thread['sandbox']['type'] != 'dangerFullAccess'
            time.sleep(.2)
            checks['mcp_not_started'] = not marker.exists()
        finally:
            proc.terminate()
            try: proc.wait(timeout=5)
            except subprocess.TimeoutExpired: proc.kill(); proc.wait(timeout=5)
    print(json.dumps({'ok': all(checks.values()), 'checks': checks, 'inference_started': False, 'user_auth_read_or_copied': False}, indent=2))
    return 0 if all(checks.values()) else 1


if __name__ == '__main__':
    raise SystemExit(main())
