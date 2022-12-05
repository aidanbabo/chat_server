package main

import (
	"fmt"
	"net"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

var port uint32 = 8000

func writeThenRead(t *testing.T, conn net.Conn, write string, read ...string) {
	var (
		nbytes int
		err    error
		buf    = make([]byte, 1024)
	)

	conn.Write([]byte(write))

	concat := strings.Join(read, "")
	for len(concat) > 0 {
		conn.SetReadDeadline(time.Now().Add(2 * time.Second))
		nbytes, err = conn.Read(buf)
		if err != nil {
			t.Fatalf("Error reading from socket '%s'", err.Error())
		}

		msg := string(buf[:nbytes])
		expected := concat[:nbytes]
		if msg != expected {
			t.Fatalf("Expected '%s' but got '%s'", expected, msg)
		}
		concat = concat[nbytes:]
	}
}

func harnessed(t *testing.T, numConns int, test func(*testing.T, []net.Conn)) {
	t.Parallel()
	port := atomic.AddUint32(&port, 1)
	p := fmt.Sprintf("%d", port)
	server := NewServer(p)
	exit := make(chan struct{})
	server.SetControl(exit)

	go Run(server)
	defer close(exit)

	server.WaitForStartup()

	conns := make([]net.Conn, 0, numConns)
	for numConns > 0 {

		conn, err := net.Dial("tcp", ":"+p)
		if err != nil {
			t.Fatalf("Error connecting to server: '%s'", err.Error())
		}
		defer conn.Close()
		conns = append(conns, conn)
	}

	test(t, conns)
}

func TestBasicSuccess(t *testing.T) {
	harnessed(t, 1, func(t *testing.T, conns []net.Conn) {
		conn := conns[0]
		writeThenRead(t, conn, "REGISTER username password\n", "RESULT REGISTER 1\n")
		writeThenRead(t, conn, "LOGIN username password\n", "RESULT LOGIN 1\n")
		writeThenRead(t, conn, "CHANNELS\n", "RESULT CHANNELS\n")
		writeThenRead(t, conn, "CREATE channel\n", "RESULT CREATE channel 1\n")
		writeThenRead(t, conn, "CHANNELS\n", "RESULT CHANNELS channel\n")
		writeThenRead(t, conn, "JOIN channel\n", "RESULT JOIN channel 1\n")
		writeThenRead(t, conn, "SAY channel Here is the message.\n", "RECV username channel Here is the message.\n", "RESULT SAY channel 1\n")
	})
}

func TestNoAccount(t *testing.T) {
	harnessed(t, 1, func(t *testing.T, conns []net.Conn) {
		conn := conns[0]
		writeThenRead(t, conn, "LOGIN username password\n", "RESULT LOGIN 0\n")
	})
}

func TestWrongPassword(t *testing.T) {
	harnessed(t, 1, func(t *testing.T, conns []net.Conn) {
		conn := conns[0]
		writeThenRead(t, conn, "REGISTER username password\n", "RESULT REGISTER 1\n")
		writeThenRead(t, conn, "LOGIN username passwordn't\n", "RESULT LOGIN 0\n")
	})
}

func TestChannelsNotLoggedIn(t *testing.T) {
	harnessed(t, 1, func(t *testing.T, conns []net.Conn) {
		conn := conns[0]
		writeThenRead(t, conn, "CHANNELS\n", "RESULT CHANNELS\n")
	})
}

func TestChannelNotLoggedIn(t *testing.T) {
	harnessed(t, 1, func(t *testing.T, conns []net.Conn) {
		conn := conns[0]
		writeThenRead(t, conn, "CREATE channel\n", "RESULT CREATE channel 1\n")
	})
}

func TestChannelAlreadyExists(t *testing.T) {
	harnessed(t, 1, func(t *testing.T, conns []net.Conn) {
		conn := conns[0]
		writeThenRead(t, conn, "CREATE channel\n", "RESULT CREATE channel 1\n")
		writeThenRead(t, conn, "CREATE channel\n", "RESULT CREATE channel 0\n")
	})
}

func TestJoinNotLoggedIn(t *testing.T) {
	harnessed(t, 1, func(t *testing.T, conns []net.Conn) {
		conn := conns[0]
		writeThenRead(t, conn, "CREATE channel\n", "RESULT CREATE channel 1\n")
		writeThenRead(t, conn, "JOIN channel\n", "RESULT JOIN channel 0\n")
	})
}

func TestJoinNoSuchChannel(t *testing.T) {
	harnessed(t, 1, func(t *testing.T, conns []net.Conn) {
		conn := conns[0]
		writeThenRead(t, conn, "REGISTER username password\n", "RESULT REGISTER 1\n")
		writeThenRead(t, conn, "LOGIN username password\n", "RESULT LOGIN 1\n")
		writeThenRead(t, conn, "JOIN channel\n", "RESULT JOIN channel 0\n")
	})
}

func TestJoinChannelAlreadyMember(t *testing.T) {
	harnessed(t, 1, func(t *testing.T, conns []net.Conn) {
		conn := conns[0]
		writeThenRead(t, conn, "REGISTER username password\n", "RESULT REGISTER 1\n")
		writeThenRead(t, conn, "LOGIN username password\n", "RESULT LOGIN 1\n")
		writeThenRead(t, conn, "CREATE channel\n", "RESULT CREATE channel 1\n")
		writeThenRead(t, conn, "JOIN channel\n", "RESULT JOIN channel 1\n")
		writeThenRead(t, conn, "JOIN channel\n", "RESULT JOIN channel 0\n")
	})
}

func TestSayNotLoggedIn(t *testing.T) {
	harnessed(t, 1, func(t *testing.T, conns []net.Conn) {
		conn := conns[0]
		writeThenRead(t, conn, "SAY channel Here is the message.\n", "RESULT SAY channel 0\n")
	})
}

func TestSayNoSuchChannel(t *testing.T) {
	harnessed(t, 1, func(t *testing.T, conns []net.Conn) {
		conn := conns[0]
		writeThenRead(t, conn, "REGISTER username password\n", "RESULT REGISTER 1\n")
		writeThenRead(t, conn, "LOGIN username password\n", "RESULT LOGIN 1\n")
		writeThenRead(t, conn, "SAY channel Here is the message.\n", "RESULT SAY channel 0\n")
	})
}

func TestSayNotChannelMember(t *testing.T) {
	harnessed(t, 1, func(t *testing.T, conns []net.Conn) {
		conn := conns[0]
		writeThenRead(t, conn, "REGISTER username password\n", "RESULT REGISTER 1\n")
		writeThenRead(t, conn, "LOGIN username password\n", "RESULT LOGIN 1\n")
		writeThenRead(t, conn, "CREATE channel\n", "RESULT CREATE channel 1\n")
		writeThenRead(t, conn, "SAY channel Here is the message.\n", "RESULT SAY channel 0\n")
	})
}

/*
func TestTwoDistributedLogin(t *testing.T) {
	t.Run("Register For Each Other", func(t *testing.T) {
		harnessed(t, 2, func(t *testing.T, conns []net.Conn) {
			conn1 := conns[0]
			conn2 := conns[1]
			writeThenRead(t, conn1, "REGISTER user1 password1\n", "RESULT REGISTER 1\n")
			writeThenRead(t, conn2, "REGISTER user2 password2\n", "RESULT REGISTER 1\n")

			writeThenRead(t, conn1, "LOGIN user2 password2\n", "RESULT LOGIN 1\n")
			writeThenRead(t, conn2, "LOGIN user1 password1\n", "RESULT LOGIN 1\n")
		})
	})
}
*/
