package main

import (
	"fmt"
	"os"
)

var muxes = []Mux{hashicorp, libp2p, xtaci, h2}

func main() {
	for _, m := range muxes {
		fmt.Fprintln(os.Stderr, "peers/go:", m.Name)
		run(m)
	}
}
