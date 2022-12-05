package main

import (
	"fmt"
	"log"
	"os"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintf(os.Stderr, "Incorrect number of command line arguments\n")
		fmt.Fprintf(os.Stderr, "Usage: './brerver <port> [<config>]'")
		os.Exit(1)
	}

	var config string
	if len(os.Args) == 3 {
		bytes, err := os.ReadFile(os.Args[2])
		if err != nil {
			log.Fatalln("Failed to read configuration file: " + err.Error())
		}
		config = string(bytes)
	}

	server := NewServer(os.Args[1])
	RunWithConfig(server, config)
}
