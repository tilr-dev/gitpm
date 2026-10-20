"""UC-008: Auth check local and empty repositories (P0).

`gitpm auth check` runs git ls-remote: readable repositories (including empty ones)
succeed without cloning, installing or registering; failures give a sanitized hint (no
raw Git stderr). The whole fixture — workspace, catalog, repositories with their Git
internals — is compared after every call.
"""

import socket
import threading

from gitpmtest import T, case

AUTH = "git-safety/read-only-repository-access-check"

ID = "dan/hello"
OK = "Read access confirmed"


def check_ok(t: T, source):
    before = t.snap(t.root, git=True)
    r = t.gitpm_ok("auth", "check", source)
    t.has(r.out, OK, "Nothing was installed")
    t.same(before, "workspace, catalog and repositories")
    return r


@case(fixture="g1_ws_catalog", spec=[AUTH])
def nonempty_bare_repository(t: T):
    """A bare clone with releases is readable."""
    b = t.root / "remotes/hello.git"
    t.run("git", "clone", "-q", "--bare", t.repo(ID), b, check=True)
    check_ok(t, b)
    check_ok(t, f"file://{b}")


@case(fixture="g1_ws_catalog", spec=[AUTH])
def empty_bare_repository(t: T):
    """An empty bare repository (no refs) is readable."""
    b = t.root / "remotes/empty.git"
    t.run("git", "init", "-q", "--bare", b, check=True)
    t.eq(t.git(b, "for-each-ref"), "", "no refs")
    check_ok(t, b)


@case(fixture="g1_ws_catalog", spec=[AUTH])
def author_working_repository(t: T):
    """A non-bare author repository path (relative, from cwd) is readable."""
    check_ok(t, "./repos/dan/hello")


@case(fixture="g1_ws_catalog", spec=[AUTH])
def missing_path_fails_sanitized(t: T):
    """A missing path fails with a useful hint and no raw Git stderr."""
    before = t.snap(t.root, git=True)
    r = t.gitpm_err("auth", "check", t.root / "remotes/missing.git", kind="auth",
                 contains="Repository not found")
    t.lacks(r.err, "fatal:", "does not appear to be a git repository")
    t.same(before, "workspace, catalog and repositories")
    t.ok(not (t.root / "remotes").exists(), "nothing created for the missing path")


def invalid_case(name: str, source: str, contains: str, doc: str):
    def fn(t: T):
        before = t.snap(t.root, git=True)
        t.gitpm_err("auth", "check", source, kind="invalid_argument", contains=contains)
        t.same(before, "workspace, catalog and repositories")
    fn.__name__, fn.__doc__, fn.__module__ = name, doc, __name__
    case(fixture="g1_ws_catalog", spec=[AUTH, "git-safety/no-option-injection"])(fn)


invalid_case("source_without_scheme", "repo-without-scheme", "full Git URL",
             "A bare word that is neither a URL nor an existing path.")
invalid_case("unsupported_protocol", "ftp://example.invalid/repo.git", "Unsupported Git URL scheme",
             "An unsupported URL scheme.")
invalid_case("url_without_host", "https:///repo.git", "has no host", "A URL without a host.")
invalid_case("remote_helper", "ext::sh -c id", "remote helpers", "A Git remote helper.")
invalid_case("empty_source", "", "Specify a Git URL", "An empty source.")


@case(fixture="g1_ws_catalog", spec=[AUTH, "cli/command-set"])
def auth_check_needs_check_subcommand(t: T):
    """`auth login URL` is a usage error naming the right form."""
    before = t.snap(t.root, git=True)
    r = t.gitpm("auth", "login", t.repo(ID))
    t.ok(r.code != 0, "refused")
    t.has(r.err, "use: gitpm auth check GIT_URL")
    t.same(before, "fixture")


@case(fixture="g1_ws_catalog", spec=[AUTH, "git-safety/git-never-waits-for-input#access-check-ls-remote"])
def auth_check_against_a_silent_server(t: T):
    """A server that accepts the connection and sends nothing: killed after 10 s (kind git)."""
    server = socket.socket()
    server.bind(("127.0.0.1", 0))
    server.listen()
    held = []
    threading.Thread(target=lambda: held.extend(iter(server.accept, None)), daemon=True).start()
    source = f"git://127.0.0.1:{server.getsockname()[1]}/silent.git"
    before = t.snap(t.root, git=True)
    # The timeout only guards the run; gitpm must stop on its own 10-second limit.
    t.gitpm_err("auth", "check", source, kind="git", contains="timed out after 10s", timeout=60)
    t.same(before, "workspace, catalog and repositories")
    server.close()
