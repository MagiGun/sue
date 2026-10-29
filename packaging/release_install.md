## Installing

**Windows:** `Sue-{version}-windows-x64-setup.exe` is the easiest (`x86` for 32-bit PCs, `arm64` for ARM ones). The `.msi` is the same installer for managed PCs, and the `-portable.zip` runs from any folder. Or install with [Scoop](https://scoop.sh):

```
scoop install https://github.com/MagiGun/sue/releases/latest/download/sue.json
```

Sue is not code-signed yet, so Windows may say "Windows protected your PC" or "Unknown publisher". Choose **More info**, then **Run anyway**. Before extracting the portable zip, right-click it, choose **Properties** and tick **Unblock**, or Windows asks again for every program inside. Scoop installs without the warning.

**macOS:** the `.dmg`. **Linux:** the AppImage runs almost anywhere; there are also a Flatpak, `.deb`, `.rpm`, an Arch package and a tarball. **Android:** the `.apk`. **FreeBSD** and **iPhone and iPad** (to sign with your own Apple account) have packages too. More at https://magigun.github.io/sue/.
