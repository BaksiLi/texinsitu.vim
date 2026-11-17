#!/usr/bin/env python3
"""Exercise the Vim installer offline, with replayed release downloads."""
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
VIM = os.environ.get("TEXINSITU_VIM", "vim")
WINDOWS = os.name == "nt"
NAME = "texinsitu.exe" if WINDOWS else "texinsitu"
ARCH = {"arm64": "aarch64", "aarch64": "aarch64", "x86_64": "x86_64",
        "amd64": "x86_64"}[platform.machine().lower()]
TARGET = (f"{ARCH}-apple-darwin" if sys.platform == "darwin" else
          "x86_64-pc-windows-msvc" if WINDOWS else f"{ARCH}-unknown-linux-musl")
ARCHIVE = f"texinsitu-v9.8.7-{TARGET}" + (".zip" if WINDOWS else ".tar.gz")
BASE = "https://github.com/BaksiLi/texinsitu.vim/releases/download/v9.8.7/"
PAYLOAD = b"MZ\x00\x01new release binary\r\n\xff\x00\n"
OLD = b"existing working binary\n"


def vim_string(value):
    return "'" + str(value).replace("'", "''") + "'"


class InstallerTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="texinsitu-install-")
        self.addCleanup(self.temporary.cleanup)
        self.work = Path(self.temporary.name).resolve()
        self.repo = self.work / "plugin space ' $; 測試"
        self.repo.mkdir()
        shutil.copytree(ROOT / "autoload", self.repo / "autoload")
        shutil.copytree(ROOT / "plugin", self.repo / "plugin")
        (self.repo / "kernel").mkdir()
        (self.repo / "kernel/Cargo.toml").write_text(
            '[package]\nname = "texinsitu"\nversion = "9.8.7"\n', encoding="utf-8",
        )
        self.destination = self.repo / "kernel/target/release" / NAME
        self.destination.parent.mkdir(parents=True)
        self.destination.write_bytes(OLD)
        self.destination.chmod(0o755)
        self.fixture = self.work / "release"
        self.fixture.mkdir()
        self.environment = os.environ.copy()
        self.make_archive()

    def make_archive(self, member=NAME, payload=PAYLOAD, kind="file"):
        archive = self.fixture / ARCHIVE
        if WINDOWS:
            with zipfile.ZipFile(archive, "w") as output:
                output.writestr(member, payload)
        else:
            with tarfile.open(archive, "w:gz") as output:
                entry = tarfile.TarInfo(member)
                entry.mode = 0o755
                if kind == "symlink":
                    entry.type = tarfile.SYMTYPE
                    entry.linkname = self.destination.as_posix()
                    output.addfile(entry)
                else:
                    entry.size = len(payload)
                    output.addfile(entry, io.BytesIO(payload))
        checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
        (self.fixture / "SHA256SUMS").write_text(
            f"{checksum}  {ARCHIVE}\n", encoding="ascii",
        )

    def run_vim(self, body, failure="", setup=""):
        result = self.work / "result"
        script = self.work / "test.vim"
        script.write_text(
            "set nocompatible encoding=utf-8\n"
            f"execute 'set runtimepath^='.fnameescape({vim_string(self.repo.as_posix())})\n"
            "runtime autoload/texinsitu/install.vim\n"
            "let s:sid = filter(getscriptinfo(), "
            "'v:val.name =~# \"autoload[/\\\\\\\\]texinsitu[/\\\\\\\\]install.vim$\"')[0].sid\n"
            "let g:Target = function('<SNR>'.s:sid.'_Target')\n"
            f"let g:fixture = {vim_string(self.fixture.as_posix())}\n"
            f"let g:destination = {vim_string(self.destination.as_posix())}\n"
            f"let g:failure = {vim_string(failure)}\n"
            "let g:downloads = []\n"
            "execute 'function! <SNR>'.s:sid.\"_Download(url, path, stage) abort\\n"
            "call add(g:downloads, a:url)\\n"
            "let l:name = fnamemodify(a:url, ':t')\\n"
            "if l:name ==# g:failure\\n"
            "call writefile(['partial download'], a:path)\\n"
            "throw 'replayed download failure'\\nendif\\n"
            "call writefile(readfile(g:fixture.'/'.l:name, 'b'), a:path, 'b')\\n"
            "endfunction\"\n"
            + setup + "\ntry\n" + body + "\ncatch\n"
            "call assert_report(v:exception.' at '.v:throwpoint)\n"
            "endtry\n"
            f"call writefile(v:errors, {vim_string(result.as_posix())})\n"
            "if !empty(v:errors) | cquit | endif\nqa!\n",
            encoding="utf-8",
        )
        completed = subprocess.run(
            [VIM, "-Nu", "NONE", "-i", "NONE", "-n", "-es", "-S", str(script)],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            env=self.environment, timeout=90,
        )
        errors = result.read_text(encoding="utf-8") if result.exists() else "Vim did not finish"
        self.assertEqual(completed.returncode, 0, errors + completed.stdout + completed.stderr)
        self.assertEqual(errors, "")
        self.assertEqual(list(self.destination.parent.glob(".texinsitu-install-*")), [])

    def expect_failure(self, message, **options):
        self.run_vim(
            "try\ncall texinsitu#install#run()\ncall assert_report('installer succeeded')\n"
            f"catch /{message}/\nendtry", **options,
        )
        self.assertEqual(self.destination.read_bytes(), OLD)

    def test_installs_the_matching_release_and_replaces_only_after_verification(self):
        self.run_vim(
            "call assert_equal(1, texinsitu#install#run())\n"
            "call assert_true(executable(g:destination))\n"
            f"call assert_equal([{vim_string(BASE + 'SHA256SUMS')}, "
            f"{vim_string(BASE + ARCHIVE)}], g:downloads)",
        )
        self.assertEqual(self.destination.read_bytes(), PAYLOAD)

    def test_first_install_creates_the_target_directory(self):
        shutil.rmtree(self.repo / "kernel/target")
        self.run_vim("call assert_equal(1, texinsitu#install#run())")
        self.assertEqual(self.destination.read_bytes(), PAYLOAD)

    def test_install_command_activates_the_current_buffer_immediately(self):
        (self.repo / "autoload/texinsitu.vim").write_text(
            "function! texinsitu#enter() abort\n"
            "let g:entered = [&l:filetype, getfsize(g:destination)]\n"
            "endfunction\n", encoding="utf-8",
        )
        for filetype in ("tex", "markdown"):
            with self.subTest(filetype=filetype):
                self.destination.unlink()
                self.run_vim(
                    f"noautocmd setfiletype {filetype}\n"
                    "runtime plugin/texinsitu.vim\n"
                    "TexInsituInstall\n"
                    f"call assert_equal(['{filetype}', {len(PAYLOAD)}], g:entered)",
                )

    def test_download_failure_keeps_the_installed_binary(self):
        for name in ("SHA256SUMS", ARCHIVE):
            with self.subTest(download=name):
                self.expect_failure("replayed download failure", failure=name)

    def test_checksum_mismatch_keeps_the_installed_binary(self):
        with (self.fixture / ARCHIVE).open("ab") as archive:
            archive.write(b"tampered")
        self.expect_failure("checksum mismatch")

    def test_missing_or_duplicate_checksum_keeps_the_installed_binary(self):
        checksum = self.fixture / "SHA256SUMS"
        original = checksum.read_text(encoding="ascii")
        for manifest in ("", original * 2, original.replace(ARCHIVE, "another-archive")):
            with self.subTest(manifest=manifest):
                checksum.write_text(manifest, encoding="ascii")
                self.expect_failure("missing or duplicate checksum")

    def test_archive_without_a_nonempty_binary_keeps_the_installed_binary(self):
        self.make_archive(payload=b"")
        self.expect_failure("did not contain")
        self.make_archive(member="some-other-file")
        self.expect_failure("TeXinsitu:")

    @unittest.skipIf(WINDOWS, "Unix archive symlinks")
    def test_archive_symlink_is_rejected(self):
        self.make_archive(kind="symlink")
        self.expect_failure("did not contain")

    def test_corrupt_archive_with_a_valid_hash_keeps_the_installed_binary(self):
        archive = self.fixture / ARCHIVE
        archive.write_bytes(b"not an archive")
        checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
        (self.fixture / "SHA256SUMS").write_text(f"{checksum}  {ARCHIVE}\n", encoding="ascii")
        self.expect_failure("TeXinsitu:")

    def test_platform_selection_and_unsupported_platforms(self):
        self.run_vim(
            "for [os, arch, target] in ["
            "['Darwin', 'arm64', 'aarch64-apple-darwin'],"
            "['Darwin', 'x86_64', 'x86_64-apple-darwin'],"
            "['Linux', 'aarch64', 'aarch64-unknown-linux-musl'],"
            "['Linux', 'x86_64', 'x86_64-unknown-linux-musl'],"
            "['Windows', 'AMD64', 'x86_64-pc-windows-msvc']]\n"
            "call assert_equal(target, g:Target(os, arch))\nendfor\n"
            "for [os, arch] in [['Linux', 'i686'], ['FreeBSD', 'x86_64'], ['Windows', 'ARM64']]\n"
            "try\ncall g:Target(os, arch)\ncall assert_report('unsupported platform accepted')\n"
            "catch /no prebuilt kernel/\nendtry\nendfor\n"
            "call assert_equal([], g:downloads)",
        )

    def test_unsupported_platform_explains_source_build_without_downloading(self):
        self.expect_failure(
            "no prebuilt kernel.*source.*requires Rust",
            setup="execute 'function! <SNR>'.s:sid.\"_Platform(stage) abort\\n"
                  "return g:Target('FreeBSD', 'x86_64')\\nendfunction\"",
        )

    def test_opening_a_buffer_does_not_install_anything(self):
        self.destination.unlink()
        self.run_vim(
            "runtime plugin/texinsitu.vim\n"
            "call setline(1, '$x^2$')\nsetfiletype tex\n"
            "call assert_false(texinsitu#enabled())\n"
            "call assert_equal([], g:downloads)",
        )
        self.assertFalse(self.destination.exists())

    def test_invalid_version_fails_without_downloading(self):
        (self.repo / "kernel/Cargo.toml").write_text(
            '[package]\nversion = "../../bad"\n', encoding="ascii",
        )
        self.expect_failure("cannot read the kernel version")

    def fake_cargo(self, fail=False):
        tooling = self.work / "tools"
        tooling.mkdir()
        build = tooling / "build.py"
        log = tooling / "arguments.json"
        build.write_text(
            "import json, pathlib, sys\n"
            f"pathlib.Path({str(log)!r}).write_text(json.dumps(sys.argv[1:]))\n"
            "stage = pathlib.Path(sys.argv[sys.argv.index('--target-dir') + 1])\n"
            "(stage / 'release').mkdir()\n"
            f"(stage / 'release' / {NAME!r}).write_bytes({PAYLOAD!r})\n"
            + ("sys.exit(19)\n" if fail else ""),
            encoding="utf-8",
        )
        launcher = tooling / ("cargo.cmd" if WINDOWS else "cargo")
        launcher.write_text(
            f'@"{sys.executable}" "{build}" %*\n' if WINDOWS else
            f"#!/bin/sh\nexec {shlex.quote(sys.executable)} {shlex.quote(str(build))} \"$@\"\n",
            encoding="utf-8",
        )
        launcher.chmod(0o755)
        self.environment["PATH"] = str(tooling) + os.pathsep + os.environ["PATH"]
        return log

    def test_explicit_source_build_uses_a_staged_locked_build_without_downloads(self):
        log = self.fake_cargo()
        self.run_vim(
            "call assert_equal(1, texinsitu#install#run('source'))\n"
            "call assert_equal([], g:downloads)",
        )
        self.assertEqual(self.destination.read_bytes(), PAYLOAD)
        arguments = json.loads(log.read_text())
        self.assertEqual(arguments[:5], ["build", "--release", "--locked", "--manifest-path",
                                        (self.repo / "kernel/Cargo.toml").as_posix()])
        self.assertEqual(arguments[5], "--target-dir")
        self.assertEqual(Path(arguments[6]).parent, self.destination.parent)

    def test_failed_source_build_keeps_the_installed_binary(self):
        self.fake_cargo(fail=True)
        self.run_vim(
            "try\ncall texinsitu#install#run('source')\ncall assert_report('build succeeded')\n"
            "catch /cargo failed/\nendtry\ncall assert_equal([], g:downloads)",
        )
        self.assertEqual(self.destination.read_bytes(), OLD)


if __name__ == "__main__":
    unittest.main()
