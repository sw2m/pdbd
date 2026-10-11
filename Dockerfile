# The CI-runner environment (contract: README "Local development").
#
# Built + run ONLY when the target runner isn't already a disposable, nix-capable
# box. GHA's ephemeral runner matches the desired env (and caches nix via the
# marketplace action), so it skips this image and runs `nix develop -c lefthook`
# directly. A dev machine isn't disposable, so it runs through this image instead —
# podman gives the isolation, and Docker's layer cache stands in for GHA's nix cache.
#
# The image is *capable* of the privileged datapath checks; whether a run gets that
# privilege is gated by podman at run time:
#   routine hooks:   podman run <img> lefthook run quality          # unprivileged
#   troubleshooting: podman run --privileged --systemd=always <img> # datapath/systemd
FROM docker.io/nixos/nix:latest

RUN mkdir -p /etc/nix && printf 'experimental-features = nix-command flakes\n' >> /etc/nix/nix.conf

# Toolchain layer — cached. The lock is COPY'd before the build so a flake.lock bump
# (and only that) busts this layer. This pre-realizes the slow, stable toolchain
# closure; the crate build stays at run time, where its repro surfaces in the suite.
WORKDIR /pdbd
ENV LEFTHOOK_CONFIG=ci/lefthook.yml
COPY flake.nix flake.lock ./
RUN nix develop --frozen -c true

# Entry under `nix develop --frozen`: every hook runs with the pinned toolchain, and
# the --frozen lock check surfaces as part of the run (this is the "nix wraps the
# build" guarantee). Usage: `podman run <img> lefthook run <hook>`.
ENTRYPOINT ["nix", "develop", "--frozen", "-c"]

# NOTE (prototype): systemd-as-PID1 capability for the privileged datapath checks is
# the piece still needing the #31 de-risking on a nixos/nix base; marked, not wired.
# A `nix profile install nixpkgs#systemd` + init wiring (or a NixOS-init base) lands
# with that prototype. Routine unprivileged hooks don't exercise this path.
