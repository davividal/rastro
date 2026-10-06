#!/usr/bin/env python3
"""Sends RESP2 commands to one server and keeps each raw reply, byte for byte.

Usage: resp.py (--unix PATH | --tcp PORT) [--password-file FILE] OUT COMMAND...
Each COMMAND is one string, split on spaces; its reply is written to OUT/<n>-<command>.resp.
`AUTH` is sent first where a password file is given, and its reply kept like any other.

Run as root, so it reaches only what a capture needs: TCP on loopback, a unix socket inside a
process's own root, writes under /captures and passwords from /root/cells.
"""

import argparse
import os
import pathlib
import re
import socket
import sys

DEADLINE_SECONDS = 10
LOOPBACK = "127.0.0.1"
CAPTURES = "/captures/"
CELLS = "/root/cells/"
# A server's socket, reached through its own root so a container's path means its own.
SOCKET_IN_A_ROOT = re.compile(r"/proc/\d+/root/[A-Za-z0-9._/-]+")


def inside(path, directory):
    """The path, resolved, where it is inside `directory`; refused otherwise."""
    resolved = os.path.realpath(path)
    if not resolved.startswith(directory):
        raise SystemExit(f"{path} is outside {directory}")
    return resolved


def encode(words):
    parts = [f"*{len(words)}\r\n".encode()]
    for word in words:
        data = word.encode()
        parts.append(f"${len(data)}\r\n".encode() + data + b"\r\n")
    return b"".join(parts)


class Reader:
    def __init__(self, sock):
        self.sock = sock
        self.buffer = b""

    def fill(self):
        chunk = self.sock.recv(65536)
        if not chunk:
            raise EOFError("the server closed the connection")
        self.buffer += chunk

    def line(self):
        while b"\r\n" not in self.buffer:
            self.fill()
        line, _, self.buffer = self.buffer.partition(b"\r\n")
        return line + b"\r\n"

    def exactly(self, count):
        while len(self.buffer) < count:
            self.fill()
        data, self.buffer = self.buffer[:count], self.buffer[count:]
        return data

    def reply(self):
        """One whole reply, as the bytes the server sent."""
        head = self.line()
        kind, body = head[:1], head[1:-2]
        if kind in (b"+", b"-", b":"):
            return head
        if kind == b"$":
            length = int(body)
            return head if length < 0 else head + self.exactly(length + 2)
        if kind == b"*":
            count = int(body)
            return head + b"".join(self.reply() for _ in range(max(count, 0)))
        raise ValueError(f"not a RESP2 reply: {head!r}")


def main():
    parser = argparse.ArgumentParser()
    target = parser.add_mutually_exclusive_group(required=True)
    target.add_argument("--unix")
    target.add_argument("--tcp", type=int, metavar="PORT")
    parser.add_argument("--password-file")
    parser.add_argument("out")
    parser.add_argument("commands", nargs="+")
    arguments = parser.parse_args()

    if arguments.unix:
        if not SOCKET_IN_A_ROOT.fullmatch(arguments.unix) or ".." in arguments.unix:
            raise SystemExit(f"{arguments.unix} is not a socket inside a process's root")
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(DEADLINE_SECONDS)
        sock.connect(arguments.unix)
    else:
        sock = socket.create_connection((LOOPBACK, arguments.tcp), timeout=DEADLINE_SECONDS)

    out = pathlib.Path(inside(arguments.out, CAPTURES))
    out.mkdir(parents=True, exist_ok=True)
    reader = Reader(sock)
    commands = [command.split(" ") for command in arguments.commands]
    if arguments.password_file:
        # One word, the default account's password, or two, an account and its password.
        credential = pathlib.Path(inside(arguments.password_file, CELLS)).read_text().split()
        commands.insert(0, ["AUTH", *credential])

    for number, words in enumerate(commands, start=1):
        sock.sendall(encode(words))
        reply = reader.reply()
        name = "AUTH" if words[0] == "AUTH" else "-".join(words).replace("*", "star")
        (out / f"{number:02d}-{name}.resp").write_bytes(reply)

    sock.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
