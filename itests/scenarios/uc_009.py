"""UC-009: Auth URL validation and failure diagnostics (P1).

REAL checks (real git, loopback network only): credentials in HTTP userinfo/query/fragment
are rejected before Git starts; a closed loopback port and a stalled loopback server fail
within a bound; a 401 server with no credential helper fails without a terminal prompt
even on a TTY with GIT_TERMINAL_PROMPT unset by the caller; raw Git stderr carrying a
synthetic marker is not echoed.

Real SSH transport checks use a local SSH helper running git-upload-pack.

SIMULATED checks: a fake `git` first on PATH records the ls-remote invocation and its
environment (SSH policy, prompt suppression, stdin) and can emit a scripted stderr; all
other git calls are passed to the real git. No real SSH server or remote is contacted.
"""

import http.server
import json
import os
import socket
import threading
import time

from gitpmtest import T, case


MARKER = "SYNTHMARKER7c1e"
BATCH_SSH = "ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=4 -o StrictHostKeyChecking=accept-new"

FAKE_GIT = r'''#!/usr/bin/env python3
import json, os, sys
argv = sys.argv[1:]
try:
    stdin = os.readlink("/proc/self/fd/0")
except OSError:
    stdin = "?"
with open(os.environ["GITPMT_GIT_LOG"], "a") as f:
    f.write(json.dumps({"argv": argv, "stdin": stdin, "stdin_tty": os.isatty(0),
                        "env": {k: v for k, v in os.environ.items()
                                if k.startswith("GIT_") or k in ("LC_ALL", "SSH_ASKPASS")}}) + "\n")
if "ls-remote" in argv:
    err = os.environ.get("GITPMT_FAKE_STDERR")
    if err:
        sys.stderr.write(err)
        sys.exit(128)
    sys.exit(0)
os.execv("/usr/bin/git", ["git"] + argv)
'''


def fake_git(t: T, **extra) -> dict:
    d = t.root / "fakebin"
    d.mkdir(exist_ok=True)
    (d / "git").write_text(FAKE_GIT)
    (d / "git").chmod(0o755)
    log = t.root / "git-calls.jsonl"
    log.touch()
    return {"PATH": f"{d}:{os.environ['PATH']}", "GITPMT_GIT_LOG": str(log), **extra}


def git_calls(t: T) -> list[dict]:
    f = t.root / "git-calls.jsonl"
    return [json.loads(l) for l in f.read_text().splitlines()] if f.exists() else []


def ls_remote_call(t: T) -> dict:
    calls = [c for c in git_calls(t) if "ls-remote" in c["argv"]]
    t.eq(len(calls), 1, "ls-remote invocations")
    return calls[0]


# ---------------------------------------------------------------- real: URL validation

def sensitive_case(name: str, url: str, contains: str):
    def fn(t: T):
        env = fake_git(t)
        before = t.snap(t.root, git=True)
        r = t.gitpm_err("auth", "check", url, kind="invalid_argument", contains=contains, env=env)
        t.lacks(r.text, MARKER)
        t.eq([c["argv"] for c in git_calls(t) if "ls-remote" in c["argv"]], [], "git ls-remote never started")
        t.same(before, "fixture")
    fn.__name__ = name
    fn.__doc__ = f"REAL: {url} is rejected before Git starts; the marker is not printed."
    fn.__module__ = __name__
    case(fixture="g1_ws_catalog", spec=["git-safety/credentials-in-urls-are-the-user-s-responsibility#token-in-url"])(fn)


for _name, _url, _contains in [
    ("https_userinfo_password", f"https://user:{MARKER}@example.invalid/r.git", "Do not put a user name or token"),
    ("https_userinfo_token", f"https://{MARKER}@example.invalid/r.git", "Do not put a user name or token"),
    ("https_query_token", f"https://example.invalid/r.git?token={MARKER}", "Do not put a user name or token"),
    ("https_fragment", f"https://example.invalid/r.git#{MARKER}", "Do not put a user name or token"),
    ("http_userinfo", f"http://user:{MARKER}@127.0.0.1/r.git", "Do not put a user name or token"),
    ("http_query", f"http://127.0.0.1/r.git?access_token={MARKER}", "Do not put a user name or token"),
    ("ssh_password", f"ssh://git:{MARKER}@example.invalid/r.git", "Do not put a password"),
]:
    sensitive_case(f"rejects_{_name}", _url, _contains)


# ---------------------------------------------------------------- real: loopback failures

@case(fixture="g1_ws_catalog", spec=["git-safety/read-only-repository-access-check"])
def closed_port_fails_fast(t: T):
    """REAL: a closed loopback port fails quickly with a network hint."""
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
    s.close()
    before = t.snap(t.root, git=True)
    start = time.time()
    r = t.gitpm_err("auth", "check", f"http://127.0.0.1:{port}/r.git", kind="auth", contains="unreachable")
    t.ok(time.time() - start < 30, f"bounded ({time.time() - start:.1f}s)")
    t.lacks(r.err, "fatal:", "curl")
    t.same(before, "fixture")


@case(fixture="g1_ws_catalog", spec=["git-safety/git-never-waits-for-input"])
def stalled_server_is_bounded(t: T):
    """REAL: a loopback server that accepts and never answers is abandoned within a bound."""
    srv = socket.socket()
    srv.bind(("127.0.0.1", 0))
    srv.listen(8)
    held = []
    threading.Thread(target=lambda: [held.append(srv.accept()) for _ in range(8)], daemon=True).start()
    start = time.time()
    r = t.gitpm("auth", "check", f"http://127.0.0.1:{srv.getsockname()[1]}/r.git", timeout=150)
    elapsed = time.time() - start
    t.ok(r.code != 0, "fails")
    t.has(r.err, "Error kind: auth")
    t.ok(elapsed < 90, f"bounded by the low-speed limit ({elapsed:.1f}s)")
    t.lacks(r.err, "fatal:")


class _Unauthorized(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(401)
        self.send_header("WWW-Authenticate", 'Basic realm="private"')
        self.send_header("Content-Length", "0")
        self.end_headers()

    def log_message(self, *_):
        pass


@case(fixture="g1_ws_catalog", spec=["git-safety/git-never-waits-for-input#credential-prompt"])
def no_terminal_prompt_on_401(t: T):
    """REAL: a 401 repository on a TTY, caller's GIT_TERMINAL_PROMPT unset: no prompt, auth hint."""
    httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), _Unauthorized)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    url = f"http://127.0.0.1:{httpd.server_address[1]}/private.git"
    start = time.time()
    r = t.run("env", "-u", "GIT_TERMINAL_PROMPT", "-u", "GIT_ASKPASS", "-u", "SSH_ASKPASS",
              "script", "-qec", f"gitpm auth check {url}", "/dev/null", input="", timeout=90)
    t.ok(r.code != 0, f"fails (exit {r.code})")
    t.ok(time.time() - start < 60, "did not wait for input")
    t.lacks(r.text, "Username for", "Password for")
    t.has(r.text, "Error kind: auth", "credential helper")


@case(fixture="g1_ws_catalog", spec=["git-safety/read-only-repository-access-check"])
def raw_stderr_not_leaked_real(t: T):
    """REAL: git's stderr naming a missing path that contains the marker is not echoed."""
    r = t.gitpm_err("auth", "check", t.root / f"{MARKER}/repo.git", kind="auth")
    t.lacks(r.text, MARKER, "fatal:")


# ---------------------------------------------------------------- simulated: fake git

@case(fixture="g1_ws_catalog", spec=["git-safety/read-only-repository-access-check#unknown-host-key"])
def raw_stderr_not_leaked_simulated(t: T):
    """SIMULATED: git stderr carrying the marker becomes a sanitized hint."""
    for stderr, hint in [
        (f"fatal: Authentication failed for 'https://example.invalid/r.git?token={MARKER}'\n", "credential helper"),
        (f"remote: Repository not found. ({MARKER})\nfatal: repository not found\n", "Repository not found"),
        (f"ssh: Could not resolve hostname {MARKER}.invalid: Name or service not known\n", "DNS"),
        (f"weird failure {MARKER}\n", "stderr is hidden"),
    ]:
        env = fake_git(t, GITPMT_FAKE_STDERR=stderr)
        r = t.gitpm_err("auth", "check", "https://example.invalid/r.git", kind="auth", contains=hint, env=env)
        t.lacks(r.text, MARKER, "fatal:")


@case(fixture="g1_ws_catalog", spec=["git-safety/git-never-waits-for-input#access-check-ls-remote", "git-safety/git-never-waits-for-input#batch-ssh-command"])
def default_ssh_policy_is_batch(t: T):
    """SIMULATED: default ls-remote gets unified noninteractive SSH, no prompts, closed stdin."""
    env = fake_git(t, GIT_TERMINAL_PROMPT="1")
    t.gitpm_ok("auth", "check", "git@example.invalid:dan/private.git", env=env)
    call = ls_remote_call(t)
    t.ok("GIT_SSH_COMMAND" not in call["env"], "no environment override")
    t.has(" ".join(call["argv"]), "core.sshCommand=" + BATCH_SSH)
    t.eq(call["env"].get("GIT_TERMINAL_PROMPT"), "0", "GIT_TERMINAL_PROMPT (caller had 1)")
    t.eq(call["stdin"], "/dev/null", "stdin")
    t.eq(call["argv"][-3:], ["ls-remote", "--", "git@example.invalid:dan/private.git"], "argv")
    t.has(" ".join(call["argv"]), "http.lowSpeedLimit=1", "http.lowSpeedTime=20")


@case(fixture="g1_ws_catalog", spec=["git-safety/git-never-waits-for-input#probe-of-the-user-s-ssh-setting"])
def explicit_git_ssh_command_kept(t: T):
    """SIMULATED: a caller GIT_SSH_COMMAND is passed through unchanged."""
    custom = "ssh -i /keys/id_test -o StrictHostKeyChecking=accept-new"
    env = fake_git(t, GIT_SSH_COMMAND=custom)
    t.gitpm_ok("auth", "check", "ssh://git@example.invalid/dan/private.git", env=env)
    call = ls_remote_call(t)
    t.eq(call["env"].get("GIT_SSH_COMMAND"), custom, "GIT_SSH_COMMAND")
    t.ok(not any(a.startswith("core.sshCommand=") for a in call["argv"]), "no batch SSH command injected")


@case(fixture="g1_ws_catalog", spec=["git-safety/git-never-waits-for-input#probe-of-the-user-s-ssh-setting"])
def explicit_git_ssh_kept(t: T):
    """SIMULATED: a caller GIT_SSH program is kept and no GIT_SSH_COMMAND is forced over it."""
    env = fake_git(t, GIT_SSH="/usr/local/bin/my-ssh")
    t.gitpm_ok("auth", "check", "git@example.invalid:dan/private.git", env=env)
    call = ls_remote_call(t)
    t.eq(call["env"].get("GIT_SSH"), "/usr/local/bin/my-ssh", "GIT_SSH")
    t.ok("GIT_SSH_COMMAND" not in call["env"], "no GIT_SSH_COMMAND injected")
    t.ok(not any(a.startswith("core.sshCommand=") for a in call["argv"]), "no batch SSH command injected")


@case(fixture="g1_ws_catalog", spec=["git-safety/git-never-waits-for-input#probe-of-the-user-s-ssh-setting"])
def core_ssh_command_kept(t: T):
    """SIMULATED: core.sshCommand in the user's Git config is respected (no GIT_SSH_COMMAND injected)."""
    cfg = t.root / "gitconfig-ssh"
    cfg.write_text(open(os.environ["GIT_CONFIG_GLOBAL"]).read() + '[core]\n\tsshCommand = "ssh -F /my/config"\n')
    env = fake_git(t, GIT_CONFIG_GLOBAL=str(cfg))
    t.gitpm_ok("auth", "check", "git@example.invalid:dan/private.git", env=env)
    call = ls_remote_call(t)
    t.ok("GIT_SSH_COMMAND" not in call["env"], "no GIT_SSH_COMMAND injected")
    t.ok(not any(a.startswith("core.sshCommand=") for a in call["argv"]), "no batch SSH command injected")
    t.eq(t.run("git", "config", "--get", "core.sshCommand", env={"GIT_CONFIG_GLOBAL": str(cfg)}).out.strip(),
         "ssh -F /my/config", "user config intact")


def real_custom_ssh_install(t: T, variable: str):
    helper = t.root / "local-ssh"
    log = t.root / "ssh.log"
    helper.write_text('#!/bin/sh\ncase "$1" in -G) exit 1;; esac\nprintf "%s\\n" "$*" >> "$GITPMT_SSH_LOG"\nexec git-upload-pack "$GITPMT_SSH_REPO"\n')
    helper.chmod(0o755)
    env = {variable: str(helper), "GITPMT_SSH_LOG": str(log), "GITPMT_SSH_REPO": str(t.repo("dan/hello"))}
    if variable == "GIT_SSH_COMMAND":
        env["GIT_SSH"] = "/nonexistent-must-not-be-used"
    source = "git@example.invalid:repo.git"
    t.gitpm_ok("auth", "check", source, env=env)
    before = len(log.read_text().splitlines())
    t.ok(before > 0, "auth uses SSH helper")
    t.gitpm_ok("install", source, env=env)
    t.ok(len(log.read_text().splitlines()) > before, "install uses SSH helper")
    t.eq(t.package("dan/hello")["source"], source, "recorded source")
    t.ok(t.dist("dan/hello").is_dir(), "package deployed")


@case(fixture="catalog", spec=["git-safety/one-subprocess-boundary#ssh-setting-from-the-environment"])
def real_git_ssh_install(t: T):
    real_custom_ssh_install(t, "GIT_SSH")


@case(fixture="catalog", spec=["git-safety/one-subprocess-boundary#ssh-setting-from-the-environment"])
def real_git_ssh_command_install(t: T):
    real_custom_ssh_install(t, "GIT_SSH_COMMAND")
