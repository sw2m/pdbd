# The CI-runner environment (contract: README "Local development").
#
# Option B: a systemd-distro base (ubi-init, default CMD /sbin/init) with nix layered
# in for the toolchain. Built + run only when the runner isn't already a disposable,
# nix-capable box — GHA's ephemeral runner matches and runs nix+lefthook directly; a
# dev machine runs through this image (podman) for isolation.
#
# One image, two run modes (privilege gated by podman at run time); the entrypoint picks:
#   datapath (privileged):   podman run --systemd=always --privileged <img>   # no args -> systemd
#   checks   (unprivileged): podman run <img> lefthook run <hook>             # a command -> devShell
FROM docker.io/nixos/nix:latest AS nix

FROM registry.access.redhat.com/ubi9/ubi-init:latest
COPY --from=nix /nix /nix
RUN mkdir -p /etc/nix && printf 'experimental-features = nix-command flakes\nbuild-users-group =\n' > /etc/nix/nix.conf
ENV PATH="/nix/var/nix/profiles/default/bin:${PATH}"
WORKDIR /pdbd
ENV LEFTHOOK_CONFIG=ci/lefthook.yml
# toolchain layer — cached; lock copied before the build so only a flake.lock bump busts
# it. `path:` treats the dir as a plain (non-git) flake — needed once the worktree mounts.
COPY flake.nix flake.lock ./
RUN nix develop --no-update-lock-file "path:/pdbd" -c true

# No args (or init) -> boot systemd (datapath mode); any command -> run it in the pinned
# devShell. So `--systemd=always` boots systemd, and `… lefthook run <hook>` runs a check.
RUN printf '#!/bin/sh\ncase "${1:-init}" in init|/sbin/init) exec /sbin/init ;; esac\nexec nix develop --no-update-lock-file "path:/pdbd" -c "$@"\n' > /usr/local/bin/pdbd-entry \
 && chmod +x /usr/local/bin/pdbd-entry
ENTRYPOINT ["/usr/local/bin/pdbd-entry"]
