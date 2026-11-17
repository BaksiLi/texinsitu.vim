#!/usr/bin/env python3
"""Prepare and verify the five native kernel release archives."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[2]
TARGETS = (
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-musl",
    "x86_64-unknown-linux-musl",
    "x86_64-pc-windows-msvc",
)


def version():
    with (ROOT / "kernel/Cargo.toml").open("rb") as manifest:
        return tomllib.load(manifest)["package"]["version"]


def archive_name(target):
    suffix = ".zip" if "windows" in target else ".tar.gz"
    return f"texinsitu-v{version()}-{target}{suffix}"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def smoke(binary):
    request = {"id": "release", "lines": ["$x^2$"], "filetype": "tex"}
    result = subprocess.run(
        [str(binary)], input=json.dumps(request) + "\n",
        text=True, encoding="utf-8", capture_output=True, check=True, timeout=15,
    )
    reply = json.loads(result.stdout)
    assert reply["id"] == "release", reply
    assert len(reply["formulas"]) == 1, reply
    assert "".join(piece[0] for piece in reply["formulas"][0]["segments"]) == "x²", reply


def prepare():
    requested = os.environ.get("RELEASE_TAG", "")
    tag = requested or f"v{version()}"
    assert re.fullmatch(r"v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", tag), tag
    assert tag == f"v{version()}", f"{tag} does not match Cargo.toml {version()}"
    commit = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True,
    ).strip()
    if requested:
        tagged = subprocess.check_output(
            ["git", "rev-parse", f"refs/tags/{tag}^{{commit}}"], cwd=ROOT, text=True,
        ).strip()
        assert commit == tagged, "checkout is not the requested tag"
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        output.write(f"tag={tag}\ncommit={commit}\npublish={'true' if requested else 'false'}\n")


def rust_notices():
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--locked", "--offline", "--format-version", "1",
        "--manifest-path", str(ROOT / "kernel/Cargo.toml"),
    ], text=True, encoding="utf-8"))
    files = []
    for crate in metadata["packages"]:
        if crate["id"] == metadata["resolve"]["root"]:
            continue
        directory = Path(crate["manifest_path"]).parent
        notices = {path for pattern in ("LICENSE*", "COPYING*", "COPYRIGHT*")
                   for path in directory.glob(pattern) if path.is_file()}
        if crate.get("license_file"):
            notices.add(directory / crate["license_file"])
        assert notices, f"No license notices found for {crate['name']} {crate['version']}"
        for notice in sorted(notices):
            member = f"LICENSES/rust/{crate['name']}-{crate['version']}/{notice.name}"
            files.append((notice, member))
    return files


def package():
    target = os.environ["TARGET"]
    assert target in TARGETS, target
    name = "texinsitu.exe" if "windows" in target else "texinsitu"
    binary = ROOT / "kernel/target" / target / "release" / name
    smoke(binary)
    if "linux" in target:
        headers = subprocess.check_output(["readelf", "-l", str(binary)], text=True)
        assert "INTERP" not in headers, "Linux release must be statically linked"
    destination = ROOT / "dist"
    destination.mkdir(exist_ok=True)
    archive = destination / archive_name(target)
    files = [(binary, name), (ROOT / "LICENSE", "LICENSE"),
             (ROOT / "THIRD_PARTY.md", "THIRD_PARTY.md")]
    files += [(notice, "LICENSES/" + notice.name) for notice in sorted((ROOT / "LICENSES").iterdir())]
    files += rust_notices()
    if "windows" in target:
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as output:
            for source, member in files:
                output.write(source, member)
    else:
        with tarfile.open(archive, "w:gz") as output:
            for source, member in files:
                output.add(source, arcname=member)
    with tempfile.TemporaryDirectory() as staging:
        executable = Path(staging) / name
        if "windows" in target:
            with zipfile.ZipFile(archive) as source:
                executable.write_bytes(source.read(name))
        else:
            with tarfile.open(archive) as source:
                executable.write_bytes(source.extractfile(name).read())
            executable.chmod(0o755)
        smoke(executable)
    archive.with_name(archive.name + ".sha256").write_text(
        f"{digest(archive)}  {archive.name}\n", encoding="utf-8",
    )
    print(archive.name)


def verify():
    destination = ROOT / "dist"
    expected = {archive_name(target) for target in TARGETS}
    actual = {path.name for path in destination.iterdir()}
    assert actual == expected | {name + ".sha256" for name in expected}, actual
    checksums = []
    for name in sorted(expected):
        archive = destination / name
        expected_line = f"{digest(archive)}  {name}\n"
        checksum = archive.with_name(name + ".sha256")
        assert checksum.read_text(encoding="utf-8") == expected_line, name
        checksums.append(expected_line)
    (destination / "SHA256SUMS").write_text("".join(checksums), encoding="utf-8")
    for name in expected:
        (destination / (name + ".sha256")).unlink()
    print("Verified all five archives and wrote SHA256SUMS")


if __name__ == "__main__":
    {"prepare": prepare, "package": package, "verify": verify}[sys.argv[1]]()
