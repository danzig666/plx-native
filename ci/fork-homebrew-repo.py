#!/usr/bin/env python3
"""Write this FORK's Homebrew Channel repository file (`repo.json`) for one fork release.

The fork publishes its stable package (`com.beb.plxnative`) as a GitHub Release; the Homebrew
Channel installs and updates it from a repository file in the format webosbrew's own listings use:
`{"packages": [{"id", "title", "shortDescription", "iconUri", "manifest": {...}}]}`, the manifest
naming the `.ipk`, its sha256 and its size. `.github/workflows/sideload-build.yml` runs this after
the release exists and commits the result to the fork's `homebrew` branch, whose raw URL is the
one a viewer adds to the Channel.

The Channel offers an update only when the manifest version is higher than the installed one, so a
fork release is stamped `<major>.<minor>.<fork number>` (`v0.8.0-fork.5` -> `0.8.5`) by
`fork_version`, which the package job applies with `ci/bump-version.py` before it builds.

Usage:
    ci/fork-homebrew-repo.py version TAG
    ci/fork-homebrew-repo.py repo TAG IPK OWNER/REPO ICON_URI > repo.json
"""
import hashlib
import json
import os
import re
import sys

TAG_RE = re.compile(r"^v(\d+)\.(\d+)\.(\d+)-fork\.([1-9]\d*)$")


def fork_version(tag: str) -> str:
    """`v0.8.0-fork.5` -> `0.8.5`: the upstream major.minor and the fork number."""
    m = TAG_RE.match(tag)
    if not m:
        raise SystemExit(f"'{tag}' is not a v<major>.<minor>.<patch>-fork.<n> tag")
    major, minor, _patch, n = m.groups()
    return f"{major}.{minor}.{n}"


def repo(tag: str, ipk: str, slug: str, icon_uri: str) -> dict:
    with open(ipk, "rb") as fh:
        data = fh.read()
    name = os.path.basename(ipk)
    app_id = name.split("_", 1)[0]
    version = fork_version(tag)
    if f"_{version}_" not in name:
        raise SystemExit(f"{name} does not carry the stamped version {version}")
    source = f"https://github.com/{slug}"
    return {
        "packages": [
            {
                "id": app_id,
                "title": "PlxNative (danzig666 fork)",
                "shortDescription": "Native Plex client with this fork's extras: automatic intro and "
                "credits skipping and a media line in the player. Replaces PlxNative.",
                "iconUri": icon_uri,
                "manifest": {
                    "id": app_id,
                    "version": version,
                    "type": "native",
                    "title": "PlxNative",
                    "iconUri": icon_uri,
                    "sourceUrl": source,
                    "rootRequired": False,
                    "ipkUrl": f"{source}/releases/download/{tag}/{name}",
                    "ipkHash": {"sha256": hashlib.sha256(data).hexdigest()},
                    "ipkSize": len(data),
                },
            }
        ]
    }


def main(argv: list) -> None:
    if len(argv) == 3 and argv[1] == "version":
        print(fork_version(argv[2]))
    elif len(argv) == 6 and argv[1] == "repo":
        print(json.dumps(repo(*argv[2:]), indent=2))
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main(sys.argv)
