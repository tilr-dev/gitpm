"""UC-049: Private Git success and revoked access (P1, optional infrastructure).

A real authenticated Git server: /usr/lib/git-core/git-http-backend behind a loopback
HTTP server that demands Basic auth (user `pmt`, a test-only password). Credentials
reach Git only through a credential helper given in the environment
(GIT_CONFIG_COUNT/credential.helper); revocation = the server's password changes.
The container-wide GIT_TERMINAL_PROMPT=0 is removed for these cases, so "no prompt"
is gitpm's own behavior; TTY variants run under script(1).
"""

import base64
import json
import os
import shlex
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from gitpmtest import R, T, case

ID = "dan/hello"
USER = "pmt"
SECRET = "s3cret-Pmt-0451"
NEW_SECRET = "rotated-Pmt-9917"
PROJECTS = Path("/fx/private")
BACKEND = "/usr/lib/git-core/git-http-backend"


class PrivateGit:
    """Loopback Git smart-HTTP server with Basic auth in front of git-http-backend."""

    def __init__(self, password: str):
        self.password = password
        self.log: list[tuple[str, str, str]] = []
        outer = self

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.0"

            def log_message(self, *_):
                pass

            def serve(self, method: str):
                path, _, query = self.path.partition("?")
                given = self.headers.get("Authorization", "")
                good = "Basic " + base64.b64encode(f"{USER}:{outer.password}".encode()).decode()
                verdict = "ok" if given == good else ("bad" if given else "none")
                outer.log.append((method, path, verdict))
                body = self.rfile.read(int(self.headers.get("Content-Length") or 0)) if method == "POST" else b""
                if verdict != "ok":
                    self.send_response(401)
                    self.send_header("WWW-Authenticate", 'Basic realm="pmt private git"')
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return
                env = {"GIT_PROJECT_ROOT": str(PROJECTS), "GIT_HTTP_EXPORT_ALL": "1", "REQUEST_METHOD": method,
                       "PATH_INFO": path, "QUERY_STRING": query, "REMOTE_USER": USER, "REMOTE_ADDR": "127.0.0.1",
                       "CONTENT_TYPE": self.headers.get("Content-Type", ""), "CONTENT_LENGTH": str(len(body)),
                       "GATEWAY_INTERFACE": "CGI/1.1", "SERVER_PROTOCOL": "HTTP/1.0", "PATH": os.environ["PATH"],
                       "GIT_CONFIG_GLOBAL": os.environ.get("GIT_CONFIG_GLOBAL", "/dev/null"), "GIT_CONFIG_NOSYSTEM": "1"}
                for header, var in [("Git-Protocol", "GIT_PROTOCOL"), ("Content-Encoding", "HTTP_CONTENT_ENCODING")]:
                    if self.headers.get(header):
                        env[var] = self.headers[header]
                out = subprocess.run([BACKEND], input=body, env=env, capture_output=True, timeout=60).stdout
                sep = b"\r\n\r\n" if b"\r\n\r\n" in out else b"\n\n"
                head, _, payload = out.partition(sep)
                status, headers = 200, []
                for line in head.decode("latin-1").splitlines():
                    name, _, value = line.partition(":")
                    if name.lower() == "status":
                        status = int(value.split()[0])
                    elif name:
                        headers.append((name, value.strip()))
                self.send_response(status)
                for name, value in headers:
                    self.send_header(name, value)
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

            def do_GET(self):
                self.serve("GET")

            def do_POST(self):
                self.serve("POST")

        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.httpd.server_address[1]}/hello.git"
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()


def setup(t: T, password: str = SECRET) -> PrivateGit:
    os.environ.pop("GIT_TERMINAL_PROMPT", None)
    PROJECTS.mkdir()
    t.run("git", "clone", "-q", "--bare", t.repo(ID), PROJECTS / "hello.git", check=True)
    server = PrivateGit(password)
    print(f"  private Git server (simulated hosting, real git-http-backend): {server.url}")
    return server


def helper(password: str = SECRET) -> dict:
    """Credential helper in the environment only (never in a URL, argument or file of gitpm)."""
    return {"GIT_CONFIG_COUNT": "1", "GIT_CONFIG_KEY_0": "credential.helper",
            "GIT_CONFIG_VALUE_0": f"!f() {{ echo username={USER}; echo password={password}; }}; f"}


def publish(t: T, version: str) -> str:
    commit = t.release(ID, version)
    t.git(ID, "push", "-q", str(PROJECTS / "hello.git"), "main", "--tags")
    return commit


def timed(t: T, *args, env=None) -> tuple[R, float]:
    start = time.monotonic()
    r = t.gitpm(*args, env=env, timeout=60)
    took = time.monotonic() - start
    print(f"  took {took:.1f}s")
    return r, took


def on_tty(t: T, *args, env=None) -> tuple[R, float]:
    """gitpm with a pseudo-terminal on stdin/stdout (script -qec), stdin closed afterwards."""
    command = shlex.join(["gitpm", *map(str, args)])
    start = time.monotonic()
    r = t.run("script", "-qec", command, "/dev/null", input="", env=env, timeout=60)
    took = time.monotonic() - start
    print(f"  took {took:.1f}s on a TTY")
    return r, took


def prompted(text: str) -> bool:
    """A Git credential prompt line (`could not read Username …: terminal prompts
    disabled` is Git refusing to prompt, not a prompt)."""
    return any(l.strip().startswith(("Username for", "Password for")) for l in text.splitlines())


def no_secret(t: T, *texts: str):
    for text in texts:
        t.lacks(text, SECRET, NEW_SECRET, base64.b64encode(f"{USER}:{SECRET}".encode()).decode())


def secrets_nowhere_on_disk(t: T):
    for root in [t.ws, t.catalog]:
        if not root.exists():
            continue
        for p in root.rglob("*"):
            if p.is_file() and not p.is_symlink() and "objects" not in p.parts:
                data = p.read_bytes()
                t.ok(SECRET.encode() not in data and NEW_SECRET.encode() not in data, f"no secret in {p}")


@case(fixture="hello", spec=["git-safety/credentials-in-urls-are-the-user-s-responsibility"])
def credentials_allow_check_install_update_register(t: T):
    server = setup(t)
    outputs = []
    r = t.gitpm_ok("auth", "check", server.url, env=helper())
    t.has(r.out, "Read access confirmed")
    outputs.append(r.text)
    outputs.append(t.gitpm_ok("install", server.url, env=helper()).text)
    p = t.package(ID)
    t.eq((p["current"]["version"], p["source"]), ("0.1.0", server.url), "installed from the private URL")
    commit = publish(t, "0.1.1")
    outputs.append(t.gitpm_ok("update", ID, env=helper()).text)
    t.eq(t.package(ID)["current"], {"version": "0.1.1", "commit": commit}, "updated with credentials")
    t.eq(t.rev(t.clone(ID)), commit, "clone HEAD")
    outputs.append(t.gitpm_ok("register", server.url, "--ref", "v0.1.1", env=helper()).text)
    t.eq(json.loads(t.catalog_index(ID).read_text())["source"], server.url, "catalog stores the clean URL")
    outputs.append(t.gitpm_ok("list").text)
    no_secret(t, *outputs)
    secrets_nowhere_on_disk(t)
    t.lacks((t.clone(ID) / ".git/config").read_text(), USER + ":", SECRET)
    verdicts = {v for _, _, v in server.log}
    t.ok({"none", "ok"} <= verdicts, f"server demanded and received credentials ({sorted(verdicts)})")


@case(fixture="hello", spec=["git-safety/git-never-waits-for-input#credential-prompt"])
def missing_credentials_fail_without_prompting(t: T):
    server = setup(t)
    r, took = timed(t, "auth", "check", server.url)
    t.ok(r.code != 0 and took < 30, "auth check fails fast")
    t.has(r.err, "Error kind: auth", "Git was not authorized")
    r, took = timed(t, "install", server.url)
    t.ok(r.code != 0 and took < 30, "install fails fast")
    t.has(r.err, "Error kind: git")
    t.not_installed(ID)
    r, took = on_tty(t, "install", server.url)
    t.ok(took < 30, "no prompt on a TTY either")
    t.has(r.out, "Error kind: git")
    t.ok(not prompted(r.out), "no credential prompt printed")
    t.not_installed(ID)
    t.ok(all(v != "ok" for _, _, v in server.log), "never authenticated")


@case(fixture="hello", spec=["errors/failing-operations-leave-the-workspace-unchanged", "git-safety/credentials-in-urls-are-the-user-s-responsibility"])
def revoked_access_fails_update_cleanly(t: T):
    server = setup(t)
    t.gitpm_ok("install", server.url, env=helper())
    publish(t, "0.1.1")
    server.password = NEW_SECRET
    print("  password changed on the server (old credentials revoked)")
    before = t.snap(t.ws)
    r, took = timed(t, "update", ID, env=helper())
    t.ok(r.code != 0, "update refused")
    t.ok(took < 30, "within 30 s")
    t.ok("Error kind: git" in r.err or "Error kind: auth" in r.err, "kind git/auth")
    t.has(r.err, "Authentication failed")
    no_secret(t, r.text)
    t.same(before, "workspace")
    t.ok(not (t.ws / ".pm-txn").exists(), "no transaction left")
    r, took = on_tty(t, "update", ID, env=helper())
    t.ok(took < 30, "no prompt on a TTY")
    t.ok(not prompted(r.out), "no credential prompt printed")
    t.ok("Error kind: git" in r.out or "Error kind: auth" in r.out, "kind git/auth on a TTY")
    no_secret(t, r.text)
    t.same(before, "workspace")
    t.eq(t.package(ID)["current"]["version"], "0.1.0", "installed version unchanged")
    secrets_nowhere_on_disk(t)
    # Only access was the problem: the new credentials update normally.
    t.gitpm_ok("update", ID, env=helper(NEW_SECRET))
    t.eq(t.package(ID)["current"]["version"], "0.1.1", "update with the new credentials")
