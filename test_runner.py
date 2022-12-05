import argparse
import json
import socket
import subprocess
import time
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


def client(test):
    with socket.create_connection(("localhost", 8000)) as sock:
        sock.settimeout(5)
        for i, [s, r] in enumerate(test["snr"]):
            ok, msg = send_and_recv(sock, s.encode(), r.encode())
            if not ok:
                return False, f"Test {test['name']} failed on line {i+1}: {msg}"
    return True, "Test Passed!"


def run_test(lang, test):
    subprocess.run(["make", "test_server"],
                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    server = subprocess.Popen(["./chat_server", "8000"],
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    # Shouldn't raise, but just in case lets have it shut the server down
    try:
        time.sleep(lang["startup_delay"])
        ok, msg = client(test)
    finally:
        server.terminate()

    return ok, msg


def run_tests(lang, tests):
    os.chdir(lang["dir"])
    try:
        for test in tests:
            ok, msg = run_test(lang, test)
            if not ok:
                return False, msg
    finally:
        os.chdir("..")
    return True, "Passed!"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("-lang", action="append")
    parser.add_argument("-filter")
    args = parser.parse_args()

    with open("test_config.json") as f:
        config = json.load(f)

    for lang in config["languages"]:
        if args.lang is None or lang["name"] in args.lang:
            ok, msg = run_tests(lang, config["tests"])
            print(f"{lang['name']}: {msg}")
            if not ok:
                return 1


if __name__ == "__main__":
    main()
