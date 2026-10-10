# Renders the datapath harness's systemd units (ci/units/*) — real unit files
# with {{var}} placeholders for nix-store binary paths. `{{ }}` rather than the
# nixpkgs `@var@` helpers because systemd's own template syntax (name@instance)
# is full of literal `@`, which `@var@` substitution would mangle. Instance
# params (%i) pass through untouched — systemd expands them at `systemctl start`.
#
# One `systemctl start pdbd-datapath@N.target` births a fully isolated datapath
# (own netns, /30 subnet, pppd unit, RuntimeDirectory), with dependency order
# and readiness gating enforced by systemd on the on-demand instance (what
# process-compose could not do — see owner/repo#25).
pkgs:
let
  inherit (pkgs) lib;

  vars = {
    "{{ip}}" = "${pkgs.iproute2}/bin/ip";
    "{{socat}}" = "${pkgs.socat}/bin/socat";
    "{{pppd}}" = "${pkgs.ppp}/bin/pppd";
    "{{sh}}" = "${pkgs.bash}/bin/sh";
  };

  names = [
    "pdbd-netns@.service"
    "pdbd-socat@.service"
    "pdbd-pppd@.service"
    "pdbd-linkup@.service"
    "pdbd-datapath@.target"
  ];

  render = name: pkgs.writeText "unit" (builtins.replaceStrings
    (builtins.attrNames vars) (builtins.attrValues vars)
    (builtins.readFile (./units + "/${name}")));

  unitDir = pkgs.runCommand "pdbd-units" { } (''
    mkdir -p "$out"
  '' + lib.concatMapStrings (n: ''cp ${render n} "$out/${n}"'' + "\n") names);
in
{
  inherit unitDir;

  # Install/remove the templates in /run/systemd/system (tmpfs → disposable) via
  # sudo; the datapath needs CAP_NET_ADMIN (netns, pppd, TUN) regardless.
  install = pkgs.writeShellScriptBin "pdbd-units-install" ''
    set -eu
    sudo -n ${pkgs.coreutils}/bin/install -m0644 -t /run/systemd/system ${unitDir}/pdbd-*
    sudo -n ${pkgs.systemd}/bin/systemctl daemon-reload
  '';
  uninstall = pkgs.writeShellScriptBin "pdbd-units-uninstall" ''
    set -eu
    sudo -n ${pkgs.coreutils}/bin/rm -f /run/systemd/system/pdbd-*
    sudo -n ${pkgs.systemd}/bin/systemctl daemon-reload
  '';
}
