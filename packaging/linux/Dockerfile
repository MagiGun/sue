# A Linux build machine for Sue's packages, for building them from any
# system with Docker:
#
#   docker build -t sue-linux packaging/linux
#   docker run --rm -v "$PWD:/src" -v sue-target:/target sue-linux
#
# The packages land in dist/. Debian 12 keeps the AppImage working on
# older distributions too.
FROM debian:bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config curl ca-certificates file rpm desktop-file-utils \
      libasound2-dev libudev-dev libxkbcommon-dev libwayland-dev libx11-dev \
      libxcursor-dev libxi-dev libxrandr-dev libgl1-mesa-dev libgtk-3-dev \
    && rm -rf /var/lib/apt/lists/*
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
ENV PATH="/root/.cargo/bin:${PATH}" CARGO_TARGET_DIR=/target
WORKDIR /src
CMD ["bash", "-c", "rustup toolchain install && packaging/linux/build.sh"]
