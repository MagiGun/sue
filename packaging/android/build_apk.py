"""Builds Sue's Android package (APK) without Gradle.

The Rust library is built for each ABI with cargo-ndk, then the SDK's own
tools put it in a package: aapt2 links the manifest and icon, zipalign
aligns it and apksigner signs it. Phones and tablets get the same package.

    python packaging/android/build_apk.py --release
    python packaging/android/build_apk.py --abi x86_64        (an emulator)

It needs the Android SDK (ANDROID_HOME) with build-tools, a platform and
the NDK, Java's keytool, and `cargo install cargo-ndk` with the Rust
targets (`rustup target add aarch64-linux-android armv7-linux-androideabi
x86_64-linux-android`). With no keystore given it signs with the debug
key in ~/.android, making one if there is none; releases pass their own
with --keystore and the SUE_KEYSTORE_PASSWORD environment variable.
"""

import argparse
import os
import pathlib
import re
import shutil
import subprocess
import sys
import zipfile

ABIS = {
    "arm64-v8a": "aarch64-linux-android",
    "armeabi-v7a": "armv7-linux-androideabi",
    "x86_64": "x86_64-linux-android",
}
ROOT = pathlib.Path(__file__).resolve().parents[2]
HERE = pathlib.Path(__file__).resolve().parent


def newest(folder: pathlib.Path) -> pathlib.Path:
    """The child of `folder` with the highest version number."""
    def key(path: pathlib.Path):
        return [int(part) if part.isdigit() else 0 for part in re.split(r"[.\-]", path.name)]
    children = [child for child in folder.iterdir() if child.is_dir()]
    if not children:
        sys.exit(f"nothing in {folder}")
    return max(children, key=key)


def tool(folder: pathlib.Path, name: str) -> str:
    for candidate in (name, name + ".exe", name + ".bat"):
        if (folder / candidate).exists():
            return str(folder / candidate)
    sys.exit(f"{name} is not in {folder}")


def run(command, **kwargs) -> None:
    print("+", " ".join(str(part) for part in command), flush=True)
    subprocess.run([str(part) for part in command], check=True, **kwargs)


def version() -> str:
    """SUE_VERSION (set by the Release workflow from the version typed or
    the tag pushed), else Cargo.toml's."""
    if os.environ.get("SUE_VERSION"):
        return os.environ["SUE_VERSION"].lstrip("v")
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    found = re.search(r'^version\s*=\s*"([^"]+)"', text, re.MULTILINE)
    return found.group(1) if found else "0.0.0"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--abi", action="append", choices=sorted(ABIS), help="an ABI to build (default: all three)")
    parser.add_argument("--release", action="store_true", help="an optimized build")
    parser.add_argument("--sdk", default=os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT"))
    parser.add_argument("--ndk", default=os.environ.get("ANDROID_NDK_HOME"))
    parser.add_argument("--min-sdk", type=int, default=26, help="the oldest Android it runs on (26 is Android 8)")
    parser.add_argument("--target-sdk", type=int, default=35)
    parser.add_argument("--keystore", help="a release keystore (default: the debug key)")
    parser.add_argument("--alias", default="androiddebugkey")
    parser.add_argument("--out", default=str(ROOT / "dist"))
    parser.add_argument("--skip-build", action="store_true", help="package the libraries already built")
    args = parser.parse_args()

    if not args.sdk:
        default = pathlib.Path.home() / ("AppData/Local/Android/Sdk" if os.name == "nt" else "Android/Sdk")
        args.sdk = str(default)
    sdk = pathlib.Path(args.sdk)
    if not sdk.is_dir():
        sys.exit(f"no Android SDK at {sdk}: set ANDROID_HOME")
    ndk = pathlib.Path(args.ndk) if args.ndk else newest(sdk / "ndk")
    build_tools = newest(sdk / "build-tools")
    platforms = sorted(
        (path for path in (sdk / "platforms").iterdir() if (path / "android.jar").exists()),
        key=lambda path: int(re.sub(r"\D", "", path.name.split(".")[0]) or 0),
    )
    if not platforms:
        sys.exit("no Android platform installed: sdkmanager \"platforms;android-35\"")
    android_jar = platforms[-1] / "android.jar"
    abis = args.abi or list(ABIS)
    profile = "release" if args.release else "debug"
    work = ROOT / "target" / "android-package"
    shutil.rmtree(work, ignore_errors=True)
    (work / "res" / "mipmap-xxxhdpi").mkdir(parents=True)
    env = dict(os.environ, ANDROID_NDK_HOME=str(ndk), ANDROID_HOME=str(sdk))

    # The Rust library, one per ABI.
    libs = work / "lib"
    if not args.skip_build:
        command = ["cargo", "ndk", "-P", str(args.min_sdk), "-o", libs]
        for abi in abis:
            command += ["-t", abi]
        command += ["build", "-p", "sue-android"]
        if args.release:
            command.append("--release")
        run(command, cwd=ROOT, env=env)
    else:
        for abi in abis:
            built = ROOT / "target" / ABIS[abi] / profile / "libsue_android.so"
            (libs / abi).mkdir(parents=True, exist_ok=True)
            shutil.copy2(built, libs / abi / built.name)
    if args.release:
        strip = next((ndk / "toolchains" / "llvm" / "prebuilt").iterdir()) / "bin"
        for library in libs.rglob("*.so"):
            run([tool(strip, "llvm-strip"), "--strip-unneeded", library])

    # The manifest and the icon, linked against the platform.
    number = version()
    parts = [int(part) for part in re.findall(r"\d+", number)[:3]] + [0, 0, 0]
    code = parts[0] * 10000 + parts[1] * 100 + parts[2]
    manifest = (HERE / "AndroidManifest.xml").read_text(encoding="utf-8")
    manifest = manifest.replace("@VERSION_CODE@", str(max(code, 1))).replace("@VERSION_NAME@", number)
    (work / "AndroidManifest.xml").write_text(manifest, encoding="utf-8")
    shutil.copy2(ROOT / "assets" / "icons" / "sue_icon.png", work / "res" / "mipmap-xxxhdpi" / "ic_launcher.png")
    shutil.copytree(HERE / "res", work / "res", dirs_exist_ok=True)
    aapt2 = tool(build_tools, "aapt2")
    run([aapt2, "compile", "--dir", work / "res", "-o", work / "res.zip"])
    base = work / "base.apk"
    run([
        aapt2, "link", "-o", base, "-I", android_jar,
        "--manifest", work / "AndroidManifest.xml",
        "--min-sdk-version", args.min_sdk, "--target-sdk-version", args.target_sdk,
        work / "res.zip",
    ])
    with zipfile.ZipFile(base, "a", compression=zipfile.ZIP_DEFLATED) as package:
        for library in sorted(libs.rglob("*.so")):
            package.write(library, f"lib/{library.parent.name}/{library.name}")
        # The license travels with every copy.
        package.write(ROOT / "LICENSE", "assets/LICENSE")

    aligned = work / "aligned.apk"
    run([tool(build_tools, "zipalign"), "-f", "-p", "4", base, aligned])

    # Signed with the release key, or the debug key.
    password = os.environ.get("SUE_KEYSTORE_PASSWORD", "android")
    keystore = pathlib.Path(args.keystore) if args.keystore else pathlib.Path.home() / ".android" / "debug.keystore"
    if not keystore.exists():
        if args.keystore:
            sys.exit(f"no keystore at {keystore}")
        keystore.parent.mkdir(parents=True, exist_ok=True)
        keytool = shutil.which("keytool") or str(pathlib.Path(os.environ.get("JAVA_HOME", "")) / "bin" / "keytool")
        run([
            keytool, "-genkeypair", "-keystore", keystore, "-storepass", "android", "-alias", "androiddebugkey",
            "-keypass", "android", "-keyalg", "RSA", "-keysize", "2048", "-validity", "10000",
            "-dname", "CN=Android Debug,O=Android,C=US",
        ])
    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    suffix = "" if set(abis) == set(ABIS) else "-" + "-".join(abis)
    final = out / f"Sue-{number}-android{suffix}{'' if args.release else '-debug'}.apk"
    run([
        tool(build_tools, "apksigner"), "sign", "--ks", keystore, "--ks-key-alias", args.alias,
        "--ks-pass", f"pass:{password}", "--key-pass", f"pass:{password}", "--out", final, aligned,
    ])
    print(final)


if __name__ == "__main__":
    main()
