#!/usr/bin/env python3
"""Exercise installed auth commands without Google requests or real credentials."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def main():
    binary = Path(sys.argv[1]).resolve()
    root = Path.home() / "construction_side" / "voxray"
    root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="auth-check-", dir=root) as temporary:
        home = Path(temporary)
        bindir = home / "bin"
        bindir.mkdir()
        installed = bindir / "voxray"
        shutil.copy2(binary, installed)
        (home / "google-drive-client.json").write_text(json.dumps({"installed": {
            "client_id": "test-desktop-client", "client_secret": "test-desktop-value"
        }}))
        fake_browser = bindir / "open"
        fake_browser.write_text('''#!/usr/bin/env python3
import os,subprocess,sys
from urllib.parse import urlparse,parse_qs,urlencode
query=parse_qs(urlparse(sys.argv[1]).query)
assert query['code_challenge_method']==['S256']
assert len(query['code_challenge'][0])==43
assert query['access_type']==['offline']
redirect=query['redirect_uri'][0]
assert urlparse(redirect).hostname=='127.0.0.1'
state=query['state'][0] if os.environ['AUTH_CASE']=='deny' else 'forged'
callback=redirect+'?'+urlencode({'state':state,'error':'access_denied'})
code="import urllib.request,sys; urllib.request.urlopen(sys.argv[1],timeout=5)"
subprocess.Popen([sys.executable,'-c',code,callback],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
''')
        fake_browser.chmod(0o700)
        env = {**os.environ, "PATH": str(bindir) + os.pathsep + os.environ["PATH"]}

        def run(*args, success=True, case="deny"):
            result = subprocess.run([str(installed), "google-auth", *args, "--json"],
                                    env={**env, "AUTH_CASE": case}, capture_output=True,
                                    text=True, timeout=15)
            assert (result.returncode == 0) == success, result.stderr
            data = json.loads(result.stdout)
            assert "test-refresh-value" not in result.stdout + result.stderr
            assert "test-desktop-value" not in result.stdout + result.stderr
            assert "code_challenge=" not in result.stdout + result.stderr
            return data

        assert run("status")["effective"]["connected"] is False
        credentials = home / "google-drive-oauth.json"
        previous = json.dumps({"client_id": "previous", "client_secret": "previous",
                               "refresh_token": "test-refresh-value"}).encode()
        credentials.write_bytes(previous)
        credentials.chmod(0o600)
        run("login", "--non-interactive", success=False)
        assert credentials.read_bytes() == previous
        for case in ["deny", "forged"]:
            run("login", success=False, case=case)
            assert credentials.read_bytes() == previous
        run("logout")
        assert not credentials.exists()
        run("logout")
        assert run("status")["effective"]["connected"] is False
        assert (home / "google-drive-client.json").exists()
    print("Installed google-auth checks passed: missing status, denied/forged login, preserved access, local logout")


if __name__ == "__main__":
    main()
