# The CI-runner environment — see README "Local development" for the disposability
# contract and the two run modes. systemd-distro base (ubi-init) + nix layered in.
FROM docker.io/nixos/nix:latest AS nix

FROM registry.access.redhat.com/ubi9/ubi-init:latest
COPY --from=nix /nix /nix
RUN mkdir -p /etc/nix && printf 'experimental-features = nix-command flakes\nbuild-users-group =\n' > /etc/nix/nix.conf
ENV PATH="/nix/var/nix/profiles/default/bin:${PATH}"
WORKDIR /pdbd
# `path:` treats the dir as a plain (non-git) flake — needed once the worktree mounts.
COPY flake.nix flake.lock ./
RUN nix develop --no-update-lock-file "path:/pdbd" -c true

# no args (or init) -> systemd (service-test mode); a command -> run it in the devShell
RUN printf '#!/bin/sh\ncase "${1:-init}" in init|/sbin/init) exec /sbin/init ;; esac\nexec nix develop --no-update-lock-file "path:/pdbd" -c "$@"\n' > /usr/local/bin/pdbd-entry \
 && chmod +x /usr/local/bin/pdbd-entry
ENTRYPOINT ["/usr/local/bin/pdbd-entry"]
