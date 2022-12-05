import argparse
import json
import socket
import subprocess
import os


def receive_equals(left, right):
    if left == right:
        return True

    # Must end with newlines so that the split makes sense
    if not left.endswith(b"\n") or not right.endswith(b"\n"):
        return False

    lsplit = left.splitlines()
    length = len(lsplit)

    leftset = set(lsplit)
    rightset = set(right.splitlines())

    return leftset == rightset and len(leftset) == length


def recv(sock, rcv):
    msg = b""
    try:
        while len(msg) < len(rcv):
            msg += sock.recv(1024)
    except socket.timeout:
        return False, "socket timeout"

    if receive_equals(msg, rcv):
        return True, "Receives equal!"
    else:
        return False, f"Expected {rcv} Got {msg}"


def send_and_recv(sock, snd, rcv):
    # @Todo @Exception this could throw ?
    sock.sendall(snd)
    return recv(sock, rcv)


def client(test, addr):
    [_, port] = addr.rsplit(":", 1)
    with socket.create_connection(("localhost", int(port))) as sock:
        sock.settimeout(5)
        for i, [s, r] in enumerate(test["snr"]):
            ok, msg = send_and_recv(sock, s.encode(), r.encode())
            if not ok:
                return False, f"Test {test['name']} failed on cmd {i+1}: {msg}"
    return True, "Test Passed!"


def run_test(lang, test):
    server = subprocess.Popen(["./chat_server", "0"],
                              stdout=subprocess.PIPE)
    try:
        addr = next(server.stdout).decode().strip()
        ok, msg = client(test, addr)
    except StopIteration:
        return False, "Failed to read server address from standard out"
    finally:
        server.terminate()

    return ok, msg


def run_tests(lang, tests):
    os.chdir(lang["dir"])

    # Shouldn't raise, just being safe, you can never know :(
    try:
        if ARGS.v:
            print("Building the Server...", end="", flush=True)
        proc = subprocess.run(["make", "test_server"],
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        if proc.returncode != 0:
            print("FAILED >:(")
            print(proc.stderr.decode())
            exit(1)

        if ARGS.v:
            print("Built")

        for test in tests:
            if ARGS.v:
                print(f"\tRunning test: {test['name']}...", end="", flush=True)
            ok, msg = run_test(lang, test)
            if not ok:
                if ARGS.v:
                    print("FAILED DDDDDD:<<<<")
                return False, msg
            elif ARGS.v:
                print("Passed :D")
    finally:
        os.chdir("..")
    return True, "Passed!"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("-v", action="store_true")
    parser.add_argument("-lang", action="append")
    parser.add_argument("-filter")
    global ARGS
    ARGS = parser.parse_args()

    with open("test_config.json") as f:
        config = json.load(f)

    for lang in config["languages"]:
        if ARGS.lang is None or lang["name"] in ARGS.lang:
            print(f"Running {lang['name']} tests...")
            ok, msg = run_tests(lang, config["tests"])
            print(f"{lang['name']}: {msg}")
            if not ok:
                return 1


if __name__ == "__main__":
    main()
