# Declarative systemd template units for the datapath test harness, keyed by a
# numeric slot %i. One `systemctl start pdbd-datapath@N.target` births a fully
# isolated datapath — own netns, /30 subnet, pppd unit, runtime dir — with
# dependency order and readiness gating enforced by systemd on the on-demand
# instance (what process-compose could not do — see owner/repo#25).
pkgs:
let
  ip = "${pkgs.iproute2}/bin/ip";
  socat = "${pkgs.socat}/bin/socat";
  pppd = "${pkgs.ppp}/bin/pppd";
  sh = "${pkgs.bash}/bin/sh";

  units = {
    "pdbd-netns@.service" = ''
      [Unit]
      Description=pdbd datapath netns (slot %i)

      [Service]
      Type=oneshot
      RemainAfterExit=yes
      ExecStart=${ip} netns add pdbd-%i
      ExecStartPost=${ip} -n pdbd-%i link set lo up
      ExecStop=${ip} netns del pdbd-%i
    '';

    "pdbd-socat@.service" = ''
      [Unit]
      Description=pdbd PTY bridge (slot %i)

      [Service]
      RuntimeDirectory=pdbd/%i
      RuntimeDirectoryMode=0700
      ExecStart=${socat} PTY,link=/run/pdbd/%i/a,raw,echo=0 PTY,link=/run/pdbd/%i/b,raw,echo=0
    '';

    "pdbd-pppd@.service" = ''
      [Unit]
      Description=pdbd pppd peer (slot %i)
      Requires=pdbd-socat@%i.service
      After=pdbd-socat@%i.service

      [Service]
      ExecStartPre=${sh} -c 'until [ -e /run/pdbd/%i/b ]; do sleep 0.1; done'
      ExecStart=${pppd} /run/pdbd/%i/b 115200 noauth local nodetach nocrtscts novj noccp lcp-echo-interval 0 unit %i 10.0.%i.2:10.0.%i.1
    '';

    "pdbd-linkup@.service" = ''
      [Unit]
      Description=pdbd linkup TUN (slot %i)
      Requires=pdbd-pppd@%i.service pdbd-netns@%i.service
      After=pdbd-pppd@%i.service pdbd-netns@%i.service

      [Service]
      NetworkNamespacePath=/run/netns/pdbd-%i
      EnvironmentFile=/run/pdbd/linkup.env
      ExecStart=${sh} -c 'exec "$LINKUP" /run/pdbd/%i/a 10.0.%i.1 10.0.%i.2 pdbd0'
    '';

    "pdbd-datapath@.target" = ''
      [Unit]
      Description=pdbd datapath instance %i
      Requires=pdbd-linkup@%i.service
      After=pdbd-linkup@%i.service
    '';
  };

  unitDir = pkgs.runCommand "pdbd-units" { } (''
    mkdir -p "$out"
  '' + pkgs.lib.concatStrings (pkgs.lib.mapAttrsToList
    (name: text: ''cp ${pkgs.writeText "unit" text} "$out/${name}"'' + "\n")
    units));
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
