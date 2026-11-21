#!/usr/bin/env python3
"""Preview or directly merge an open GitHub PR with a Gitmoji caption."""

import argparse
import hashlib
import json
import re
import shlex
import subprocess
import sys
import unicodedata
from pathlib import Path


PR_QUERY = """
query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      id number state isDraft title
      headRefName headRefOid headRepository { nameWithOwner } headRepositoryOwner { login }
      baseRefName baseRef { target { oid } } baseRepository { nameWithOwner }
      viewerMergeHeadlineText(mergeType: MERGE)
      viewerMergeBodyText(mergeType: MERGE)
      isMergeQueueEnabled stack { number size baseRefName } stackEntry { position }
    }
  }
}
"""


def run(argv, payload=None):
    result = subprocess.run(
        argv, input=json.dumps(payload) if payload is not None else None,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, encoding="utf-8", check=False,
    )
    if result.returncode:
        raise ValueError(result.stderr.strip() or f"{argv[0]} exited with {result.returncode}")
    return result.stdout


def api(host, endpoint, payload, method="POST"):
    raw = run(["gh", "api", "--hostname", host, "--method", method, endpoint, "--input", "-"], payload)
    result = json.loads(raw)
    if not isinstance(result, dict):
        raise ValueError("GitHub returned an unexpected API response")
    return result


def read_pr(args):
    owner, name = args.repo.split("/")
    result = api(args.hostname, "graphql", {
        "query": PR_QUERY, "variables": {"owner": owner, "name": name, "number": args.pr},
    })
    if result.get("errors"):
        raise ValueError("GitHub could not read the pull request: " + str(result["errors"]))
    pr = ((result.get("data") or {}).get("repository") or {}).get("pullRequest")
    if not pr or pr["number"] != args.pr:
        raise ValueError(f"cannot read PR #{args.pr} in {args.repo} on {args.hostname}")
    if pr["state"] != "OPEN":
        raise ValueError(f"PR #{args.pr} is {pr['state'].lower()}; select an open pull request")
    return pr


def branch(repository, ref):
    return f"{repository}:{ref}"


def message_body(pr, original):
    base = pr["baseRepository"]["nameWithOwner"]
    if pr["headRepository"]:
        source = branch(pr["headRepository"]["nameWithOwner"], pr["headRefName"])
    else:
        owner = (pr["headRepositoryOwner"] or {}).get("login")
        source = f"{owner}:{pr['headRefName']}" if owner else pr["headRefName"]
        source += f" (source repository unavailable; refs/pull/{pr['number']}/head)"
    target = branch(base, pr["baseRefName"])
    context = [f"Source: {source}", f"Base: {target}"]
    if pr["stack"]:
        stack = pr["stack"]
        pos = (pr["stackEntry"] or {}).get("position")
        layer = f", layer {pos} of {stack['size']}" if pos is not None else ""
        context.append(f"Stack #{stack['number']}{layer}, targeting {branch(base, stack['baseRefName'])}")

    if original is None:
        original = pr["viewerMergeHeadlineText"]
        if pr["viewerMergeBodyText"]:
            original += "\n\n" + pr["viewerMergeBodyText"]
    provenance = "\n".join(context)
    return f"{provenance}\n\n{original}" if original else provenance


def caption(pr):
    title = pr["title"]
    if not title.strip() or any(unicodedata.category(c) in {"Cc", "Zl", "Zp"} for c in title):
        raise ValueError("the pull request needs a nonempty, single-line title")
    # A PR's baseRefOid can outlive the branch tip at which it was recorded.
    base = ((pr["baseRef"] or {}).get("target") or {}).get("oid")
    head = pr["headRefOid"]
    if not all(isinstance(oid, str) and re.fullmatch(r"[a-fA-F0-9]{40}|[a-fA-F0-9]{64}", oid)
               for oid in (base, head)):
        raise ValueError("GitHub did not return commit IDs for both pull request tips")
    try:
        output = run(["gitmoji", "merge-caption", "--json", "--pr", str(pr["number"]), f"--title={title}", base, head])
    except ValueError as exc:
        ref = shlex.quote("refs/heads/" + pr["baseRefName"])
        raise ValueError(f"{exc}\nFor missing objects, fetch {ref} and refs/pull/{pr['number']}/head "
                         "from the target repository into this checkout, then retry.") from exc
    inventory = json.loads(output)
    if not isinstance(inventory, dict):
        raise ValueError("gitmoji returned an invalid caption inventory")
    pr_title = inventory.get("title")
    subject = inventory.get("subject")
    if (not isinstance(pr_title, str) or not pr_title.strip()
            or any(unicodedata.category(c) in {"Cc", "Zl", "Zp"} for c in pr_title)
            or subject != f"{pr_title} (#{pr['number']})"):
        raise ValueError("gitmoji returned an invalid merge subject; use a CLI that supplies the PR title and landed subject")
    recognized, total = inventory.get("recognized"), inventory.get("total")
    if type(recognized) is not int or type(total) is not int or not 0 <= recognized <= total:
        raise ValueError("gitmoji returned invalid caption coverage")
    return subject, recognized, total


def check_direct(pr):
    if pr["stack"]:
        raise ValueError("this is a native stack; use GitHub's stacked PR merge flow")
    if pr["isMergeQueueEnabled"]:
        raise ValueError("the target branch uses a merge queue; submit this PR through its queue")
    if pr["isDraft"]:
        raise ValueError("mark the draft PR ready for review before merging")
    if not pr["headRepository"]:
        raise ValueError("the source repository is unavailable; inspect the PR in GitHub before merging")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", required=True, metavar="OWNER/REPO")
    parser.add_argument("--pr", required=True, type=int, metavar="NUMBER")
    parser.add_argument("--hostname", default="github.com", metavar="HOST")
    parser.add_argument("--original-message", type=Path, metavar="FILE", help="original UTF-8 merge text to preserve, including conflict notes")
    parser.add_argument("--merge", action="store_true", help="directly merge this PR using a merge commit")
    parser.add_argument("--expect-preview", metavar="SHA256", help="digest from the reviewed preview, required with --merge")
    args = parser.parse_args(argv)
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]*/[A-Za-z0-9_.-]+", args.repo) or args.repo.split("/")[1] in {".", ".."}:
        parser.error("--repo must be OWNER/REPO")
    if not re.fullmatch(r"[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)*", args.hostname):
        parser.error("--hostname must be a hostname, such as github.com")
    if not 0 < args.pr <= 2**31 - 1:
        parser.error("--pr must be a positive GitHub PR number")
    if args.merge and not re.fullmatch(r"[0-9a-f]{64}", args.expect_preview or ""):
        parser.error("--merge requires --expect-preview SHA256 from a reviewed preview")
    if args.expect_preview is not None and not args.merge:
        parser.error("use --expect-preview with --merge")

    original = args.original_message.read_bytes().decode("utf-8") if args.original_message else None
    pr = read_pr(args)
    if args.merge:
        check_direct(pr)
    subject, recognized, total = caption(pr)
    body = message_body(pr, original)
    payload = {"sha": pr["headRefOid"], "merge_method": "merge", "commit_title": subject, "commit_message": body}
    # Bind the two invocations even when a new head, destination or message would render the same emojis.
    snapshot = [args.hostname, args.repo, pr, payload, recognized, total]
    digest = hashlib.sha256(json.dumps(snapshot, sort_keys=True, separators=(",", ":")).encode("ascii")).hexdigest()
    if not args.merge:
        print(f"Base {pr['baseRef']['target']['oid']} / Head {pr['headRefOid']}", file=sys.stderr)
        print(f"Recognized tags in {recognized} of {total} incoming non-merge commits.", file=sys.stderr)
        print(f"Reviewed preview: --expect-preview {digest}", file=sys.stderr)
        return f"{subject}\n\n{body}"
    if args.expect_preview != digest:
        raise ValueError("the PR or proposed message differs from the reviewed preview; inspect a fresh preview before merging")

    # Re-read mutable metadata after formatting. GitHub also checks the head SHA atomically on PUT.
    if read_pr(args) != pr:
        raise ValueError("the PR changed while preparing the caption; review a fresh preview and retry")
    url = f"https://{args.hostname}/{args.repo}/pull/{args.pr}"
    try:
        result = api(args.hostname, f"repos/{args.repo}/pulls/{args.pr}/merge", payload, method="PUT")
    except (ValueError, OSError) as exc:
        raise ValueError(f"{exc}\nCheck {url} for the merge outcome before retrying.") from exc
    if result.get("merged") is not True:
        raise ValueError(result.get("message") or "GitHub did not merge this pull request")
    return f"Merged {url}" + (f" at {result['sha']}" if result.get("sha") else "")


if __name__ == "__main__":
    try:
        output = main()
        sys.stdout.buffer.write(output.encode("utf-8"))
        if not output.endswith("\n"):
            sys.stdout.buffer.write(b"\n")
    except (ValueError, OSError) as error:
        sys.exit(str(error))
