package main

import (
	"io"
	"net"
	"net/http"

	"golang.org/x/net/http2"
)

type Inbound struct {
	body io.ReadCloser
	w    http.ResponseWriter
	done chan struct{}
}

func (f *Inbound) Read(p []byte) (int, error) { return f.body.Read(p) }

func (f *Inbound) Write(p []byte) (int, error) {
	n, err := f.w.Write(p)
	f.w.(http.Flusher).Flush()
	return n, err
}

func (f *Inbound) Close() error {
	select {
	case <-f.done:
	default:
		close(f.done)
	}
	return nil
}

type Outbound struct {
	body *io.PipeWriter
	resp io.ReadCloser
}

func (f *Outbound) Read(p []byte) (int, error)  { return f.resp.Read(p) }
func (f *Outbound) Write(p []byte) (int, error) { return f.body.Write(p) }
func (f *Outbound) Close() error                { return f.body.Close() }

var h2 = Mux{"x/net/http2 v0.58.0 (go)", func(near, far net.Conn) (Session, error) {
	found := make(chan Flow, 4096)
	srv := &http2.Server{
		MaxConcurrentStreams:         4096, // the largest scenario's flow count
		MaxUploadBufferPerStream:     window,
		MaxUploadBufferPerConnection: 1 << 30, // per-flow windows are what we compare
	}
	go srv.ServeConn(far, &http2.ServeConnOpts{Handler: http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		w.(http.Flusher).Flush()
		f := &Inbound{r.Body, w, make(chan struct{})}
		found <- f
		<-f.done
	})})
	cc, err := (&http2.Transport{AllowHTTP: true}).NewClientConn(near)
	if err != nil {
		return nil, err
	}
	return Duo{
		open: func() (Flow, error) {
			pr, pw := io.Pipe()
			req, err := http.NewRequest(http.MethodPost, "http://peer/", pr)
			if err != nil {
				return nil, err
			}
			resp, err := cc.RoundTrip(req)
			if err != nil {
				return nil, err
			}
			return &Outbound{pw, resp.Body}, nil
		},
		accept: func() (Flow, error) { return <-found, nil },
		close:  func() error { cc.Close(); return far.Close() },
	}, nil
}}
