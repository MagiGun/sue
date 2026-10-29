# Security

Sue opens files from anywhere, so a file that crashes it, runs code or
reaches outside what it was asked to read is a security problem.

## Reporting

Please do not open a public issue. Report privately through GitHub's
"Report a vulnerability" button on the repository's Security tab, with the
file or steps that show the problem and the version (`sue --version`).
You will hear back within a week.

## What Sue promises

- `unsafe` code is forbidden everywhere except audited code in
  `crates/sue-platform`, and only with a maintainer's approval.
- Decoders refuse damaged or unknown input with an error; they do not panic.
- Sue works offline. It downloads and installs nothing by itself, and its AI
  features run on your own machine.
- Packages are signed (Ed25519) and installed into a new generation that
  rolls back if it fails its health check.
