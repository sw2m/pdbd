package main

import (
	"fmt"
	"io"
	"math"
	"net"
	"runtime"
	"runtime/debug"
	"sort"
	"sync"
	"time"
)

// see crates/mux/CLAUDE.md §"Benchmarks": every harness runs these same numbers
const (
	chunk  = 64 * 1024
	bulk   = 64 << 20
	window = 256 * 1024
	msg    = 64
	warm   = 100
	pings  = 2000
	churns = 1000
	idles  = 1000
	runs   = 5
)

var data = make([]byte, chunk)

type Flow = io.ReadWriteCloser

type Session interface {
	Open() (Flow, error)
	Accept() (Flow, error)
	Close() error
}

type Duo struct {
	open   func() (Flow, error)
	accept func() (Flow, error)
	close  func() error
}

func (d Duo) Open() (Flow, error)   { return d.open() }
func (d Duo) Accept() (Flow, error) { return d.accept() }
func (d Duo) Close() error          { return d.close() }

type Run struct{ Session }

type Mux struct {
	Name string
	New  func(near, far net.Conn) (Session, error)
}

func pair() (net.Conn, net.Conn) {
	l, err := net.Listen("tcp", "127.0.0.1:0")
	must(err)
	defer l.Close()
	type accepted struct {
		c   net.Conn
		err error
	}
	ch := make(chan accepted, 1)
	go func() {
		c, err := l.Accept()
		ch <- accepted{c, err}
	}()
	near, err := net.Dial("tcp", l.Addr().String())
	must(err)
	far := <-ch
	must(far.err)
	near.(*net.TCPConn).SetNoDelay(true)
	far.c.(*net.TCPConn).SetNoDelay(true)
	return near, far.c
}

func version(path string) string {
	if info, ok := debug.ReadBuildInfo(); ok {
		for _, d := range info.Deps {
			if d.Path == path {
				return d.Version
			}
		}
	}
	return "?"
}

func must(err error) {
	if err != nil {
		panic(err)
	}
}

func rate(n int, d time.Duration) float64 {
	return float64(n) / float64(1<<20) / d.Seconds()
}

func spread(rates []float64) float64 {
	n := float64(len(rates))
	mean := 0.0
	for _, r := range rates {
		mean += r
	}
	mean /= n
	v := 0.0
	for _, r := range rates {
		v += (r - mean) * (r - mean)
	}
	return 100 * math.Sqrt(v/n) / mean
}

func drain(f Flow, want int) time.Time {
	buf := make([]byte, chunk)
	got := 0
	for got < want {
		n, err := f.Read(buf)
		got += n
		if err != nil && got < want {
			panic(fmt.Sprintf("read after %d of %d: %v", got, want, err))
		}
	}
	return time.Now()
}

func fill(f Flow, want int) {
	for sent := 0; sent < want; {
		n := min(chunk, want-sent)
		_, err := f.Write(data[:n])
		must(err)
		sent += n
	}
}

func accepted(s Session, n int) chan Flow {
	ch := make(chan Flow, n)
	go func() {
		for i := 0; i < n; i++ {
			f, err := s.Accept()
			must(err)
			ch <- f
		}
	}()
	return ch
}

func (r Run) Bulk() float64 {
	s := r.Session
	in := accepted(s, 1)
	start := time.Now()
	out, err := s.Open()
	must(err)
	done := make(chan time.Time)
	go func() { done <- drain(<-in, bulk) }()
	fill(out, bulk)
	end := <-done
	out.Close()
	return rate(bulk, end.Sub(start))
}

func (r Run) Flows(n int) (float64, float64) {
	s := r.Session
	share := bulk / n
	in := accepted(s, n)
	outs := make([]Flow, n)
	for i := range outs {
		f, err := s.Open()
		must(err)
		_, err = f.Write(data[:1])
		must(err)
		outs[i] = f
	}
	ins := make([]Flow, n)
	for i := range ins {
		ins[i] = <-in
		one := make([]byte, 1)
		_, err := io.ReadFull(ins[i], one)
		must(err)
	}
	start := time.Now()
	ends := make([]time.Time, n)
	var wg sync.WaitGroup
	for i := range ins {
		wg.Add(2)
		go func(i int) { defer wg.Done(); ends[i] = drain(ins[i], share) }(i)
		go func(i int) { defer wg.Done(); fill(outs[i], share) }(i)
	}
	wg.Wait()
	last := start
	rates := make([]float64, n)
	for i, e := range ends {
		if e.After(last) {
			last = e
		}
		rates[i] = rate(share, e.Sub(start))
	}
	for _, f := range outs {
		f.Close()
	}
	return rate(share*n, last.Sub(start)), spread(rates)
}

func (r Run) Pingpong() float64 {
	s := r.Session
	in := accepted(s, 1)
	near, err := s.Open()
	must(err)
	_, err = near.Write(data[:msg])
	must(err)
	far := <-in
	go func() {
		buf := make([]byte, msg)
		for {
			if _, err := io.ReadFull(far, buf); err != nil {
				return
			}
			if _, err := far.Write(buf); err != nil {
				return
			}
		}
	}()
	buf := make([]byte, msg)
	_, err = io.ReadFull(near, buf)
	must(err)
	trips := make([]time.Duration, 0, pings)
	for i := 0; i < warm+pings; i++ {
		start := time.Now()
		_, err := near.Write(data[:msg])
		must(err)
		_, err = io.ReadFull(near, buf)
		must(err)
		if i >= warm {
			trips = append(trips, time.Since(start))
		}
	}
	near.Close()
	sort.Slice(trips, func(i, j int) bool { return trips[i] < trips[j] })
	return float64(trips[len(trips)/2].Nanoseconds()) / 1e3
}

func answer(f Flow) {
	one := make([]byte, 1)
	_, err := io.ReadFull(f, one)
	must(err)
	_, err = f.Write(one)
	must(err)
	f.Close()
}

func (r Run) Churn() float64 {
	s := r.Session
	start := time.Now()
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		for i := 0; i < churns; i++ {
			f, err := s.Accept()
			must(err)
			wg.Add(1)
			go func() { defer wg.Done(); answer(f) }()
		}
	}()
	one := make([]byte, 1)
	for i := 0; i < churns; i++ {
		f, err := s.Open()
		must(err)
		_, err = f.Write(data[:1])
		must(err)
		_, err = io.ReadFull(f, one)
		must(err)
		f.Close()
		io.Copy(io.Discard, f)
	}
	wg.Wait()
	return float64(churns) / time.Since(start).Seconds()
}

func heap() int64 {
	time.Sleep(100 * time.Millisecond) // let a previous session's goroutines exit
	runtime.GC()
	runtime.GC()
	var m runtime.MemStats
	runtime.ReadMemStats(&m)
	return int64(m.HeapAlloc + m.StackInuse)
}

func (r Run) Idle() float64 {
	s := r.Session
	before := heap()
	in := accepted(s, idles)
	held := make(chan []Flow)
	go func() {
		fs := make([]Flow, 0, idles)
		one := make([]byte, 1)
		for i := 0; i < idles; i++ {
			f := <-in
			_, err := io.ReadFull(f, one)
			must(err)
			_, err = f.Write(one)
			must(err)
			fs = append(fs, f)
		}
		held <- fs
	}()
	outs := make([]Flow, 0, idles)
	one := make([]byte, 1)
	for i := 0; i < idles; i++ {
		f, err := s.Open()
		must(err)
		_, err = f.Write(data[:1])
		must(err)
		_, err = io.ReadFull(f, one)
		must(err)
		outs = append(outs, f)
	}
	ins := <-held
	time.Sleep(50 * time.Millisecond) // let the sessions settle their buffers
	per := float64(heap()-before) / idles
	for _, f := range append(outs, ins...) {
		f.Close()
	}
	return per
}

func run(m Mux) {
	report := func(key string, v float64) { fmt.Printf("%s\t%s\t%g\n", m.Name, key, v) }
	fresh := func() Session {
		s, err := m.New(pair())
		must(err)
		return s
	}
	for i := 0; i <= runs; i++ {
		keep := i > 0
		measure := func(key string, f func(Run) float64) {
			s := fresh()
			v := f(Run{s})
			s.Close()
			if keep {
				report(key, v)
			}
		}
		measure("bulk", Run.Bulk)
		for _, n := range []int{16, 256} {
			s := fresh()
			r, sp := Run{s}.Flows(n)
			s.Close()
			if keep {
				report(fmt.Sprintf("flows%d", n), r)
				report(fmt.Sprintf("fair%d", n), sp)
			}
		}
		measure("pingpong", Run.Pingpong)
		measure("churn", Run.Churn)
		measure("idle", Run.Idle)
	}
}
