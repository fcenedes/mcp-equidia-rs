#!/usr/bin/env python3
"""Validate versions, smoke-test binaries and assemble auditable release artifacts."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent
PACKAGE = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]
NAME, VERSION = PACKAGE["name"], PACKAGE["version"]
TARGETS = ("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu", "aarch64-apple-darwin", "x86_64-apple-darwin")


def verify_tag(tag):
    if tag != f"v{VERSION}" or not re.fullmatch(r"v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", tag):
        raise ValueError(f"release tag must match Cargo.toml: v{VERSION}")
    return VERSION


def digest(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def checksum(path):
    path.with_name(path.name + ".sha256").write_text(f"{digest(path)}  {path.name}\n")


def archive_name(target):
    if target not in TARGETS:
        raise ValueError("unsupported release target")
    return f"{NAME}-v{VERSION}-{target}.tar.gz"


def verify_binary(binary):
    env = {k: v for k, v in os.environ.items() if not k.startswith(("EQUIDIA_", "MCP_"))}
    env.pop("RUST_LOG", None)
    version = subprocess.check_output([str(binary), "--version"], env=env, text=True, timeout=30)
    if version.strip() != f"{NAME} {VERSION}":
        raise ValueError("binary version does not match Cargo.toml")
    tools = json.loads(subprocess.check_output([str(binary), "--no-env-file", "tools"], env=env, timeout=30))
    manifest = json.loads((ROOT / "api/catalog.json").read_text())
    expected = {o["name"] for o in manifest["operations"]} | set(manifest["aliases"]) | {"equidia_api_catalog"}
    if {t["name"] for t in tools} != expected or len(tools) != len(expected):
        raise ValueError("binary catalogue does not match the bundled manifest")
    if not all(t["annotations"]["readOnlyHint"] and not t["annotations"]["destructiveHint"] for t in tools):
        raise ValueError("binary exposed a non-read-only tool")


def package(target, output, binary=None):
    binary = (binary or ROOT / "target" / target / "release" / NAME).resolve()
    verify_binary(binary)
    output.mkdir(parents=True, exist_ok=True)
    destination = output / archive_name(target)
    with tempfile.TemporaryDirectory() as temporary:
        folder = Path(temporary) / destination.name.removesuffix(".tar.gz")
        folder.mkdir()
        shutil.copy2(binary, folder / NAME)
        for name in ("README.md", "LICENSE", "NOTICE", "CHANGELOG.md", ".env.example"):
            shutil.copy2(ROOT / name, folder / name)
        for name in ("docs", "deploy", "api"):
            shutil.copytree(ROOT / name, folder / name)
        with tarfile.open(destination, "w:gz") as archive:
            archive.add(folder, arcname=folder.name)
    checksum(destination)
    print(f"Packaged {destination}; version and tool-discovery smoke checks passed")


def audit_package(path=None):
    path = path or ROOT / "target/package" / f"{NAME}-{VERSION}.crate"
    root_files = {"Cargo.toml", "Cargo.toml.orig", "Cargo.lock", "README.md", "LICENSE", "NOTICE", "CHANGELOG.md", ".env.example", ".cargo_vcs_info.json", "Dockerfile", ".dockerignore"}
    required = {"Cargo.toml", "Cargo.lock", "src/lib.rs", "src/main.rs", "README.md", "LICENSE", "NOTICE", "api/catalog.json", "api/openapi.json", "api/access-policy.json", "docs/crate.md", "docs/tools.md", "docs/release.md", "Dockerfile", "deploy/compose.image.yaml", "scripts/release.py"}
    seen = set()
    prefix = f"{NAME}-{VERSION}/"
    with tarfile.open(path, "r:gz") as archive:
        for member in archive.getmembers():
            if not member.isfile() or not member.name.startswith(prefix):
                raise ValueError("unexpected package entry")
            name = member.name[len(prefix):]
            parts = Path(name).parts
            if Path(name).is_absolute() or ".." in parts or name in seen:
                raise ValueError("unsafe or duplicate package path")
            if name not in root_files and not name.startswith(("src/", "api/", "docs/", "tests/", "scripts/", "deploy/")):
                raise ValueError(f"unexpected package file: {name}")
            if "__pycache__" in parts or name.endswith((".pyc", ".pyo")):
                raise ValueError("Python cache found in package")
            if any(p == ".env" or (p.startswith(".env.") and p != ".env.example") or p.endswith((".pem", ".key")) or p in {"credentials.json", "sa_key.json"} for p in parts):
                raise ValueError("credential file found in package")
            if name == ".env.example":
                content = archive.extractfile(member).read().decode()
                for line in content.splitlines():
                    key, _, value = line.partition("=")
                    if key.endswith("TOKEN") and value.strip():
                        raise ValueError("example configuration contains a token")
            seen.add(name)
    if required - seen:
        raise ValueError(f"missing package files: {sorted(required - seen)}")
    print(f"Audited {len(seen)} package files; required assets present, credential paths excluded")


def checksums(output, require_all):
    paths = sorted(output.glob(f"{NAME}-v{VERSION}-*.tar.gz.sha256"))
    expected = {archive_name(t) for t in TARGETS}
    names = {p.name.removesuffix(".sha256") for p in paths}
    if not paths or not names <= expected or (require_all and names != expected):
        raise ValueError("missing or unexpected binary archive checksums")
    lines = []
    for path in paths:
        name = path.name.removesuffix(".sha256")
        line = f"{digest(output / name)}  {name}"
        if path.read_text().strip() != line:
            raise ValueError(f"checksum mismatch: {name}")
        lines.append(line)
    (output / "SHA256SUMS").write_text("\n".join(lines) + "\n")
    print(f"Verified {len(lines)} archives")


def image_metadata(repository, tag):
    verify_tag(tag)
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("repository must be owner/name")
    image = f"ghcr.io/{repository.lower()}"
    tags = [f"{image}:{VERSION}"]
    if "-" not in VERSION:
        tags += [f"{image}:{'.'.join(VERSION.split('.')[:2])}", f"{image}:latest"]
    return {"version": VERSION, "image": image, "tags": tags}


def image_manifest(repository, tag, directory):
    metadata = image_metadata(repository, tag)
    references = []
    for arch in ("amd64", "arm64"):
        reference = (directory / f"digest-{arch}.txt").read_text().strip()
        if not re.fullmatch(re.escape(metadata["image"]) + r"@sha256:[0-9a-f]{64}", reference):
            raise ValueError("invalid container digest reference")
        references.append(reference)
    command = ["docker", "buildx", "imagetools", "create"]
    for image_tag in metadata["tags"]:
        command += ["--tag", image_tag]
    subprocess.run(command + references, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    verify = commands.add_parser("verify-tag"); verify.add_argument("tag")
    build = commands.add_parser("package"); build.add_argument("--target", choices=TARGETS, required=True)
    build.add_argument("--output", type=Path, default=ROOT / "dist"); build.add_argument("--binary", type=Path)
    audit = commands.add_parser("audit-package"); audit.add_argument("--path", type=Path)
    sums = commands.add_parser("checksums"); sums.add_argument("--output", type=Path, default=ROOT / "dist"); sums.add_argument("--require-all", action="store_true")
    meta = commands.add_parser("image-metadata"); meta.add_argument("repository"); meta.add_argument("tag")
    manifest = commands.add_parser("image-manifest"); manifest.add_argument("repository"); manifest.add_argument("tag"); manifest.add_argument("--directory", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    if args.command == "verify-tag": print(verify_tag(args.tag))
    elif args.command == "package": package(args.target, args.output, args.binary)
    elif args.command == "audit-package": audit_package(args.path)
    elif args.command == "checksums": checksums(args.output, args.require_all)
    elif args.command == "image-metadata": print(json.dumps(image_metadata(args.repository, args.tag)))
    else: image_manifest(args.repository, args.tag, args.directory)


if __name__ == "__main__":
    main()
