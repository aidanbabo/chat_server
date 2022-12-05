package main

import (
	"bytes"
	"fmt"
	"io"
	"log"
	"net"
	"strings"
	"sync"
)

type user struct {
	name          string
	conn          net.Conn
	channels      map[string]*channel
	remoteChannel chan string
}

func (u user) loggedIn() bool {
	return u.name != ""
}

type channel struct {
	usersLock sync.RWMutex
	users     map[string]*user
}

// Essentially all the global state, extracted into a struct for testing purposes
type Server struct {
	port string
	// Don't worry about one user on multiple devices idt
	usersLock sync.RWMutex
	users     map[string]string

	// Each channel has a lock so you only need to take this lock when modifying the map
	channelsLock sync.RWMutex
	channels     map[string]*channel

	serversLock sync.RWMutex
	servers     map[string]net.Conn

	// a message will be sent when the server starts and one will be received for shutdown
	control chan struct{}
}

func NewServer(port string) *Server {
	return &Server{
		port:     port,
		users:    map[string]string{},
		channels: map[string]*channel{},
		servers:  map[string]net.Conn{},
	}
}

func (s *Server) WaitForStartup() {
	if s.control != nil {
		<-s.control
	} else {
		panic("Tried to wait on server startup with no control channel")
	}
}

func (s *Server) SetControl(control chan struct{}) {
	s.control = control
}

func login(s *Server, u *user, args []string) {
	if len(args) != 3 {
		return
	}
	username := args[1]
	password := args[2]

	s.usersLock.RLock()
	defer s.usersLock.RUnlock()

	var confirmation int
	if pass, ok := s.users[username]; ok && username != "" && pass == password {
		u.name = username
		confirmation = 1
	}

	msg := fmt.Sprintf("RESULT LOGIN %d\n", confirmation)
	u.conn.Write([]byte(msg))
}

func register(s *Server, u *user, args []string) {
	if len(args) != 3 {
		return
	}
	username := args[1]
	password := args[2]

	s.usersLock.Lock()
	defer s.usersLock.Unlock()

	var confirmation int
	if _, ok := s.users[username]; !ok {
		s.users[username] = password
		confirmation = 1
	}

	msg := fmt.Sprintf("RESULT REGISTER %d\n", confirmation)
	u.conn.Write([]byte(msg))
}

func join(s *Server, u *user, args []string) {
	if len(args) != 2 {
		return
	}
	channelName := args[1]

	var confirmation int
	defer func() {
		msg := fmt.Sprintf("RESULT JOIN %s %d\n", channelName, confirmation)
		u.conn.Write([]byte(msg))
	}()

	if !u.loggedIn() {
		return
	}
	if _, ok := u.channels[channelName]; ok {
		return
	}

	s.channelsLock.RLock()
	channel, ok := s.channels[channelName]
	s.channelsLock.RUnlock()
	if !ok {
		return
	}

	channel.usersLock.Lock()
	defer channel.usersLock.Unlock()
	channel.users[u.name] = u
	u.channels[channelName] = channel
	confirmation = 1
}

func create(s *Server, u *user, args []string) {
	if len(args) != 2 {
		return
	}
	channelName := args[1]

	var confirmation int
	defer func() {
		msg := fmt.Sprintf("RESULT CREATE %s %d\n", channelName, confirmation)
		u.conn.Write([]byte(msg))
	}()

	s.channelsLock.Lock()
	defer s.channelsLock.Unlock()
	if _, ok := s.channels[channelName]; ok {
		return
	}

	s.channels[channelName] = &channel{
		users: map[string]*user{},
	}
	confirmation = 1
}

func say(s *Server, u *user, args []string) {
	if len(args) != 3 {
		return
	}
	channelName := args[1]
	message := args[2]

	var confirmation int
	defer func() {
		msg := fmt.Sprintf("RESULT SAY %s %d\n", channelName, confirmation)
		u.conn.Write([]byte(msg))
	}()

	if !u.loggedIn() {
		return
	}
	channel, ok := u.channels[channelName]
	if !ok {
		return
	}

	channel.usersLock.RLock()
	defer channel.usersLock.RUnlock()
	msg := []byte(fmt.Sprintf("RECV %s %s %s\n", u.name, channelName, message))
	for _, user := range channel.users {
		user.conn.Write(msg)
	}
	confirmation = 1
}

func listChannels(s *Server, u *user, args []string) {
	s.channelsLock.RLock()
	defer s.channelsLock.RUnlock()

	var builder bytes.Buffer
	builder.WriteString("RESULT CHANNELS")
	for name := range s.channels {
		builder.WriteRune(' ')
		builder.WriteString(name)
		builder.WriteRune(',')
	}
	if len(s.channels) > 0 {
		builder.Truncate(builder.Len() - 1)
	}
	builder.WriteRune('\n')

	bytes := builder.Bytes()
	u.conn.Write(bytes)
}

func userConnection(s *Server, conn net.Conn) {
	u := &user{
		conn:          conn,
		channels:      map[string]*channel{},
		remoteChannel: make(chan string),
	}

	defer func() {
		for _, channel := range u.channels {
			channel.usersLock.Lock()
			delete(channel.users, u.name)
			channel.usersLock.Unlock()
		}
		// Avoid closing user socket to prevent the port from staying open
		// https://stackoverflow.com/questions/880557/socket-accept-too-many-open-files
		u.conn.Close()
	}()

	connection := make(chan string)
	go func() {
		buf := make([]byte, 1024)
		for {
			nbytes, err := u.conn.Read(buf)
			if err != nil {
				if err == io.EOF {
					break
				}
				log.Fatalf("Failed to read bytes from connection: %v\n", err)
			}

			msg := string(buf[:nbytes])
			last := len(msg) - 1
			if msg[last] != '\n' {
				log.Printf("Ignoring message without newline at the end: '%s'.", msg)
				continue
			}
			msg = msg[:last] // Trime newline
			connection <- msg
		}
	}()

	for {
		select {
		case msg := <-u.remoteChannel:
			u.conn.Write([]byte(msg))
		case msg := <-connection:
			words := strings.SplitN(msg, " ", 3)
			switch words[0] {
			case "LOGIN":
				login(s, u, words)
			case "REGISTER":
				register(s, u, words)
			case "JOIN":
				join(s, u, words)
			case "CREATE":
				create(s, u, words)
			case "SAY":
				say(s, u, words)
			case "CHANNELS":
				listChannels(s, u, words)
			default:
				log.Printf("Unknown command %s\n", words[0])
			}
		}
	}
}

func Run(s *Server) {
	RunWithConfig(s, "")
}

func RunWithConfig(s *Server, config string) {
	ln, err := net.Listen("tcp", ":"+s.port)

	// For testing
	fmt.Println(ln.Addr().String())

	if err != nil {
		log.Fatalln("Failed to start TCP server: " + err.Error())
	}
	defer ln.Close()

	if s.control != nil {
		s.control <- struct{}{}
	}

	/*
		lines := strings.Split(config, "\n")
		for _, line := range lines {
			go serverConnection(server, line)
		}
	*/

	connections := make(chan net.Conn)
	go func() {
		for {
			conn, err := ln.Accept()
			if err != nil {
				log.Println("Failed to accept TCP connection: " + err.Error())
				continue
			}
			connections <- conn
		}
	}()

Loop:
	for {
		select {
		case conn := <-connections:
			go userConnection(s, conn)
		case <-s.control:
			break Loop
		}
	}
}
