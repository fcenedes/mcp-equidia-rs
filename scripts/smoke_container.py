"""Exercise the built container without Equidia credentials or live API calls."""

import argparse
import json
import select
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path


HARDENING = ["--read-only", "--cap-drop=ALL", "--security-opt=no-new-privileges"]
INITIALIZE = {
    "jsonrpc": "2.0", "id": 1, "method": "initialize",
    "params": {
        "protocolVersion": "2025-11-25", "capabilities": {},
        "clientInfo": {"name": "container-smoke", "version": "1"},
    },
}


def docker(*args, check=True):
    return subprocess.run(["docker", *args], check=check, text=True, capture_output=True, timeout=60)


def json_response(response):
    body = response.read().decode()
    if response.headers.get_content_type() == "text/event-stream":
        return next(json.loads(line[5:]) for line in body.splitlines()
                    if line.startswith("data:") and line[5:].strip())
    return json.loads(body)


def check_stdio(image, platform=None):
    name = "mcp-equidia-stdio-smoke-" + uuid.uuid4().hex
    process = subprocess.Popen(
        ["docker", "run", "--rm", "--name", name, "-i", *(["--platform", platform] if platform else []), *HARDENING, image, "serve", "stdio"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
    )
    try:
        def request(value):
            process.stdin.write(json.dumps(value) + "\n")
            process.stdin.flush()

        def response():
            assert select.select([process.stdout], [], [], 15)[0], "stdio response timed out"
            return json.loads(process.stdout.readline())

        request(INITIALIZE)
        assert response()["result"]["serverInfo"]["name"] == "mcp-equidia-rs"
        request({"jsonrpc": "2.0", "method": "notifications/initialized"})
        request({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
        assert len(response()["result"]["tools"]) == 72
        process.stdin.close()
        assert process.wait(timeout=15) == 0, process.stderr.read()
    finally:
        docker("rm", "--force", name, check=False)
        if process.poll() is None:
            process.kill()
            process.wait(timeout=15)


def check_http(image, platform=None):
    with tempfile.TemporaryDirectory() as directory:
        dotenv = Path(directory) / "runtime.env"
        dotenv.write_text(
            "MCP_HTTP_TOKEN=file-token\n"
            "MCP_HTTP_ALLOWED_HOSTS=container.example:3000\n"
            "EQUIDIA_ENABLED_TOOLS=equidia_meetings\n"
        )
        dotenv.chmod(0o644)
        container = docker(
            "run", "--detach", *(["--platform", platform] if platform else []), *HARDENING, "--publish", "127.0.0.1::3000",
            "--mount", f"type=bind,src={dotenv},dst=/app/.env,readonly",
            "--env", "MCP_HTTP_TOKEN=environment-token",
            "--env", "EQUIDIA_ENABLED_TOOLS=equidia_races", image,
        ).stdout.strip()
        try:
            port = docker("port", container, "3000/tcp").stdout.strip().rsplit(":", 1)[1]
            base = f"http://127.0.0.1:{port}"
            # Local container probes must not use a developer's outbound HTTP proxy.
            http = urllib.request.build_opener(urllib.request.ProxyHandler({}))
            deadline = time.monotonic() + 30
            while True:
                try:
                    with http.open(base + "/health", timeout=2) as response:
                        assert response.read() == b"ok"
                    break
                except (OSError, urllib.error.URLError):
                    if time.monotonic() >= deadline:
                        raise AssertionError(docker("logs", container).stderr)
                    time.sleep(0.25)

            def post(value, token="environment-token", session=None):
                headers = {
                    "Content-Type": "application/json",
                    "Accept": "application/json, text/event-stream",
                    "Host": "127.0.0.1:3000",
                    "Authorization": "Bearer " + token,
                }
                if session:
                    headers["Mcp-Session-Id"] = session
                    headers["MCP-Protocol-Version"] = "2025-11-25"
                return http.open(urllib.request.Request(
                    base + "/mcp", data=json.dumps(value).encode(), headers=headers,
                ), timeout=5)

            try:
                post(INITIALIZE, token="file-token")
            except urllib.error.HTTPError as error:
                assert error.code == 401
            else:
                raise AssertionError("dotenv token overrode the environment token")
            with post(INITIALIZE) as response:
                session = response.headers["Mcp-Session-Id"]
                assert json_response(response)["result"]["serverInfo"]["name"] == "mcp-equidia-rs"
            with post({"jsonrpc": "2.0", "method": "notifications/initialized"}, session=session) as response:
                assert response.status == 202
            with post({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}, session=session) as response:
                names = [tool["name"] for tool in json_response(response)["result"]["tools"]]
                assert names == ["equidia_races"], names
            docker("stop", "--time", "10", container)
            assert docker("inspect", "--format", "{{.State.ExitCode}}", container).stdout.strip() == "0"
        finally:
            docker("rm", "--force", container, check=False)



def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image")
    parser.add_argument("--platform", choices=["linux/amd64", "linux/arm64"])
    args = parser.parse_args()
    check_stdio(args.image, args.platform)
    check_http(args.image, args.platform)
    print("Container passed stdio, HTTP sessions, auth, filtering and graceful shutdown")


if __name__ == "__main__":
    main()
