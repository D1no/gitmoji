#!/usr/bin/env python3
"""Synchronize one open pull request's Gitmoji title in a trusted Actions job."""

import argparse
import base64
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


@dataclass(frozen=True)
class Pull:
    title: str
    base_branch: str
    base: str
    head: str


class GitHub:
    def __init__(self, number):
        if not 0 < number <= 2**31 - 1:
            raise ValueError("Use a positive GitHub pull request number.")
        self.number = number
        repo = os.environ["GITHUB_REPOSITORY"]
        if not re.fullmatch(r"[A-Za-z0-9_-]+/[A-Za-z0-9._-]+", repo) or repo.split("/")[1] in {".", ".."}:
            raise ValueError("Invalid GITHUB_REPOSITORY.")
        server = os.environ["GITHUB_SERVER_URL"].rstrip("/")
        api = os.environ["GITHUB_API_URL"].rstrip("/")
        host = urlsplit(server)
        if (host.scheme != "https" or not host.hostname or host.username is not None
                or host.password is not None or host.path or host.query or host.fragment):
            raise ValueError("GITHUB_SERVER_URL must be an HTTPS origin.")
        if api not in {f"{server}/api/v3", f"https://api.{host.netloc}"}:
            raise ValueError("GITHUB_API_URL must belong to the base repository's GitHub host.")
        self.remote = f"{server}/{repo}.git"
        self.repository_api = f"{api}/repos/{repo}"
        self.endpoint = f"{self.repository_api}/pulls/{number}"
        self.token = os.environ["GITMOJI_TITLE_TOKEN"]

    def request(self, title=None, *, endpoint=None):
        headers = {"Authorization": f"Bearer {self.token}", "Accept": "application/vnd.github+json"}
        payload = None
        if title is not None:
            headers["Content-Type"] = "application/json"
            payload = json.dumps({"title": title}).encode()
        req = Request(endpoint or self.endpoint, data=payload, headers=headers, method="GET" if title is None else "PATCH")
        try:
            with build_opener(NoRedirect()).open(req, timeout=30) as response:
                return json.load(response)
        except HTTPError as error:
            detail = " Shorten the human wording if the full title exceeds the limit, then check repository validation rules." if error.code == 422 else ""
            raise ValueError(f"GitHub rejected the PR {req.method} (HTTP {error.code}).{detail}") from error

    def read(self):
        pr = self.request()
        if pr["number"] != self.number:
            raise ValueError("GitHub returned a different PR.")
        if pr["state"] == "closed":
            return None
        if pr["state"] != "open":
            raise ValueError("Unknown PR state.")
        # The PR resource can retain an older base SHA. Read the actual branch
        # tip on each check, including the final check immediately before PATCH.
        branch = pr["base"]["ref"]
        if not isinstance(branch, str) or not branch or any(part in {"", ".", ".."} for part in branch.split("/")):
            raise ValueError("Invalid destination branch.")
        ref = self.request(endpoint=f"{self.repository_api}/git/ref/heads/{quote(branch, safe='/')}")
        if ref["ref"] != f"refs/heads/{branch}" or ref["object"]["type"] != "commit":
            raise ValueError("GitHub returned a different destination ref.")
        record = Pull(pr["title"], branch, ref["object"]["sha"], pr["head"]["sha"])
        if not all(isinstance(oid, str) and re.fullmatch(r"[0-9a-f]{40}", oid) for oid in (record.base, record.head)):
            raise ValueError("The PR's base or head commit is unavailable.")
        return record

    def update(self, title):
        if self.request(title)["title"] != title:
            raise ValueError("GitHub returned a different title. Inspect the current PR and rerun.")


def command(argv, cwd, env):
    result = subprocess.run(argv, cwd=cwd, env=env, encoding="utf-8", capture_output=True)
    if result.returncode:
        # Git error output can quote refs or subjects controlled by a contributor.
        raise ValueError(f"{Path(argv[0]).name} {argv[1]} failed (exit {result.returncode}).")
    return result.stdout


def proposal(host, pr, binary, directory):
    # Start with empty local refs and full commit ancestry. Neither the PR's tree
    # nor the caller's checkout/configuration is ever used as executable input.
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_") and k != "GITMOJI_TITLE_TOKEN"}
    env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull, GIT_TERMINAL_PROMPT="0")
    command(["git", "init", "--bare", "--quiet", "--template="], directory, env)
    branch_ref = f"refs/heads/{pr.base_branch}"
    command(["git", "check-ref-format", branch_ref], directory, env)
    basic = base64.b64encode(f"x-access-token:{host.token}".encode()).decode()
    fetch_env = dict(env, GIT_CONFIG_COUNT="1", GIT_CONFIG_KEY_0=f"http.{host.remote}.extraHeader",
                     GIT_CONFIG_VALUE_0=f"Authorization: Basic {basic}")
    try:
        command(["git", "-c", "http.followRedirects=false", "fetch", "--no-tags", "--no-recurse-submodules",
                 "--filter=tree:0", "--", host.remote, f"+{branch_ref}:refs/title/base",
                 f"+refs/pull/{host.number}/head:refs/title/head"], directory, fetch_env)
    except ValueError as error:
        raise ValueError("Could not fetch the base commit and PR head. Check contents: read and the PR refs.") from error
    revisions = command(["git", "rev-parse", "refs/title/base", "refs/title/head"], directory, env).splitlines()
    if revisions != [pr.base, pr.head]:
        raise ValueError("The fetched base or PR head differs from the GitHub API. Retry after synchronization.")
    raw = command([str(binary), "merge-caption", "--json", "--pr", str(host.number),
                   f"--title={pr.title}", pr.base, pr.head], directory, env)
    title = json.loads(raw).get("title")
    if not isinstance(title, str) or not title:
        raise ValueError("The CLI did not return a PR title. Use Gitmoji 0.14.0 or later.")
    return title


def synchronize(host, binary):
    for _ in range(3):
        pr = host.read()
        if pr is None:
            return "PR is closed."
        try:
            with tempfile.TemporaryDirectory(prefix="gitmoji-pr-title-") as directory:
                title = proposal(host, pr, binary, directory)
        except ValueError:
            if host.read() != pr:
                continue
            raise
        if host.read() != pr:
            continue
        if title == pr.title:
            return "PR title already matches."
        # The GitHub title endpoint offers no atomic expected-title argument.
        host.update(title)
        return "Updated PR title."
    raise ValueError("PR changed repeatedly during the preview. Rerun against its current state.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pr", type=int, required=True)
    parser.add_argument("--gitmoji", required=True, type=Path)
    args = parser.parse_args()
    print(synchronize(GitHub(args.pr), args.gitmoji.resolve(strict=True)))


if __name__ == "__main__":
    try:
        main()
    except (KeyError, OSError, ValueError, URLError) as error:
        sys.exit(str(error))
