#!/usr/bin/env python3
"""Mirror complete, verified AnyDrop RC releases before publishing latest.json."""

import argparse
import base64
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
from urllib.parse import quote, unquote, urlparse


TARGETS = {"windows-x86_64", "darwin-aarch64", "darwin-x86_64"}
VERSION = re.compile(r"^(\d+)\.(\d+)\.(\d+)-rc\.(\d+)\.(\d+)$")


def version_key(version):
    match = VERSION.fullmatch(version)
    if not match:
        raise ValueError("Expected an AnyDrop RC version")
    return tuple(int(part) for part in match.groups())


class GitHub:
    def __init__(self, repo):
        self.repo = repo

    def run(self, *args):
        env = {**os.environ, "GH_PROMPT_DISABLED": "1"}
        env.pop("GH_DEBUG", None)
        result = subprocess.run(
            ["gh", *args], check=True, capture_output=True, text=True, timeout=900, env=env
        )
        return result.stdout

    def releases(self):
        return json.loads(self.run("api", f"repos/{self.repo}/releases?per_page=30"))

    def download(self, tag, name, destination):
        self.run("release", "download", tag, "--repo", self.repo, "--pattern", name,
                 "--dir", str(destination))


def verify_file(file, signature, public_key):
    signature_file = file.with_name(file.name + ".sig")
    signature_file.write_bytes(base64.b64decode(signature, validate=True))
    subprocess.run(
        ["minisign", "-Vm", str(file), "-p", str(public_key), "-x", str(signature_file)],
        check=True, capture_output=True, timeout=60,
    )


def checked_manifest(data, release, repo, public_base):
    version = data.get("version", "")
    version_key(version)
    if release["tag_name"] != "rc-v" + version:
        raise ValueError("Release tag and manifest version differ")
    platforms = data.get("platforms", {})
    if set(platforms) != TARGETS:
        raise ValueError("A complete Windows and universal macOS release is required")
    if platforms["darwin-aarch64"] != platforms["darwin-x86_64"]:
        raise ValueError("Both macOS targets must use the same universal archive")
    assets = {asset["name"] for asset in release["assets"]}
    downloads = {}
    rewritten = {}
    for target, info in platforms.items():
        url = urlparse(info["url"])
        name = unquote(url.path.rsplit("/", 1)[-1])
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._+ -]*", name):
            raise ValueError("Unsafe artifact name")
        expected_path = f"/{repo}/releases/download/{release['tag_name']}/{name}"
        if (url.scheme != "https" or url.netloc != "github.com" or url.query or url.fragment
                or unquote(url.path) != expected_path or name not in assets):
            raise ValueError("Artifact must belong to this GitHub release")
        signature = info["signature"].strip()
        base64.b64decode(signature, validate=True)
        if not signature:
            raise ValueError("Missing updater signature")
        suffix = ".exe" if target == "windows-x86_64" else ".app.tar.gz"
        if not name.endswith(suffix):
            raise ValueError("Wrong updater artifact type")
        if name in downloads and downloads[name] != signature:
            raise ValueError("Conflicting signatures for the same artifact")
        downloads[name] = signature
        rewritten[target] = {
            "url": f"{public_base.rstrip('/')}/rc/{version}/{quote(name, safe='')}",
            "signature": signature,
        }
    return {**data, "platforms": rewritten}, downloads


def checksum(file):
    with file.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def mirror(output, public_base, public_key, github, verify=verify_file):
    if urlparse(public_base).scheme != "https":
        raise ValueError("The public update endpoint must use HTTPS")
    channel = output / "rc"
    channel.mkdir(parents=True, exist_ok=True)
    with (output / ".mirror.lock").open("w") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return "Another mirror process is running"
        candidates = [
            release for release in github.releases()
            if not release["draft"] and release["tag_name"].startswith("rc-v")
            and VERSION.fullmatch(release["tag_name"][4:])
            and any(asset["name"] == "latest.json" for asset in release["assets"])
        ]
        if not candidates:
            return "No signed RC release available yet"
        release = max(candidates, key=lambda item: version_key(item["tag_name"][4:]))
        version = release["tag_name"][4:]
        latest = channel / "latest.json"
        if latest.exists():
            current = json.loads(latest.read_text())
            if version_key(current["version"]) >= version_key(version):
                return f"Already serving {current['version']}"
        with tempfile.TemporaryDirectory(prefix=".staging-", dir=channel) as temporary:
            stage = Path(temporary)
            github.download(release["tag_name"], "latest.json", stage)
            data = json.loads((stage / "latest.json").read_text())
            result, downloads = checked_manifest(data, release, github.repo, public_base)
            for name, signature in downloads.items():
                github.download(release["tag_name"], name, stage)
                if not (stage / name).is_file() or not (stage / name).stat().st_size:
                    raise ValueError("Missing or empty artifact")
                verify(stage / name, signature, public_key)
            manifest_bytes = (json.dumps(result, ensure_ascii=False, indent=2) + "\n").encode()
            (stage / "latest.json").write_bytes(manifest_bytes)
            (stage / "sha256sums.txt").write_text("".join(
                f"{checksum(stage / name)}  {name}\n" for name in sorted(downloads)
            ))
            destination = channel / version
            if destination.exists():
                for file in stage.iterdir():
                    other = destination / file.name
                    if not other.is_file() or checksum(file) != checksum(other):
                        raise ValueError("Refusing to replace an existing immutable version")
            else:
                stage.chmod(0o755)
                for file in stage.iterdir():
                    file.chmod(0o644)
                stage.rename(destination)
            with tempfile.NamedTemporaryFile(prefix=".latest-", dir=channel, delete=False) as stream:
                pending = Path(stream.name)
                try:
                    stream.write(manifest_bytes)
                    stream.flush()
                    os.fsync(stream.fileno())
                    pending.chmod(0o644)
                    pending.replace(latest)
                finally:
                    pending.unlink(missing_ok=True)
        return f"Published verified RC {version} ({len(downloads)} unique artifacts)"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default="hatsune-miku/anydrop")
    parser.add_argument("--output", type=Path, default=Path("/var/www/html/anydrop-api"))
    parser.add_argument("--public-base", default="https://anydrop-api.vanillacake.cn")
    parser.add_argument("--public-key", type=Path, default=Path("/etc/anydrop-updater/minisign.pub"))
    args = parser.parse_args()
    print(mirror(args.output, args.public_base, args.public_key, GitHub(args.repo)), flush=True)


if __name__ == "__main__":
    main()
