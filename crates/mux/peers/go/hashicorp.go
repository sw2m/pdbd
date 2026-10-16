package main

import (
	"io"
	"net"

	hashi "github.com/hashicorp/yamux"
)

var hashicorp = Mux{"hashicorp/yamux v0.1.2 (go)", func(near, far net.Conn) (Session, error) {
	cfg := hashi.DefaultConfig()
	cfg.MaxStreamWindowSize = window
	cfg.AcceptBacklog = 4096 // the largest scenario's flow count
	cfg.EnableKeepAlive = false
	cfg.LogOutput = io.Discard
	c, err := hashi.Client(near, cfg)
	if err != nil {
		return nil, err
	}
	s, err := hashi.Server(far, cfg)
	if err != nil {
		return nil, err
	}
	return Duo{
		open:   func() (Flow, error) { return c.OpenStream() },
		accept: func() (Flow, error) { return s.AcceptStream() },
		close:  func() error { c.Close(); return s.Close() },
	}, nil
}}
