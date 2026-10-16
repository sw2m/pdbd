package main

import (
	"context"
	"io"
	"net"

	p2p "github.com/libp2p/go-yamux/v5"
)

var libp2p = Mux{"libp2p/go-yamux v5.1.0, auto-tuned (go)", func(near, far net.Conn) (Session, error) {
	cfg := p2p.DefaultConfig()
	cfg.InitialStreamWindowSize = window
	cfg.MaxIncomingStreams = 4096 // the largest scenario's flow count
	cfg.AcceptBacklog = 4096
	cfg.EnableKeepAlive = false
	cfg.LogOutput = io.Discard
	c, err := p2p.Client(near, cfg, nil)
	if err != nil {
		return nil, err
	}
	s, err := p2p.Server(far, cfg, nil)
	if err != nil {
		return nil, err
	}
	return Duo{
		open:   func() (Flow, error) { return c.OpenStream(context.Background()) },
		accept: func() (Flow, error) { return s.AcceptStream() },
		close:  func() error { c.Close(); return s.Close() },
	}, nil
}}
