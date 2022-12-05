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
    while len(msg) < len(rcv):
        msg += sock.recv(1024)
    assert receive_equals(msg, rcv), f"Expected {rcv} Got {msg}"


def send_and_recv(sock, snd, rcv):
    sock.sendall(snd)
    recv(sock, rcv)


def client(test):
    with socket.create_connection(("localhost", 8000)) as sock:
        sock.settimeout(5)
        for [s, r] in test["snr"]:
            send_and_recv(sock, s.encode(), r.encode())


def run_test(lang, test):
    subprocess.run(["make", "test_server"],
                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    server = subprocess.Popen(["./chat_server", "8000"],
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        time.sleep(lang["startup_delay"])
        client(test)
    finally:
        server.terminate()


def run_tests(lang, tests):
    os.chdir(lang["dir"])
    try:
        for test in tests:
            run_test(lang, test)
    finally:
        os.chdir("..")
    print(f"{lang['name']} tests pass!")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("-lang", action="append")
    parser.add_argument("-filter")
    args = parser.parse_args()

    with open("test_config.json") as f:
        config = json.load(f)

    for lang in config["languages"]:
        if lang["name"] in args.lang:
            run_tests(lang, config["tests"])


if __name__ == "__main__":
    main()
