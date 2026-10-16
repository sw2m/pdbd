package main

import (
	"net"

	"github.com/xtaci/smux"
)

var xtaci = Mux{"xtaci/smux v1.5.57, v2 (go)", func(near, far net.Conn) (Session, error) {
	cfg := smux.DefaultConfig()
	cfg.Version = 2 // per-stream windows; v1 has only the session bucket
	cfg.KeepAliveDisabled = true
	cfg.MaxFrameSize = 65535
	cfg.MaxStreamBuffer = window
	cfg.MaxReceiveBuffer = 1 << 30 // per-flow windows are what we compare, not the session's
	c, err := smux.Client(near, cfg)
	if err != nil {
		return nil, err
	}
	s, err := smux.Server(far, cfg)
	if err != nil {
		return nil, err
	}
	return Duo{
		open:   func() (Flow, error) { return c.OpenStream() },
		accept: func() (Flow, error) { return s.AcceptStream() },
		close:  func() error { c.Close(); return s.Close() },
	}, nil
}}
