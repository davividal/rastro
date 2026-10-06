#!/usr/bin/env python3
"""Sends RESP2 commands to one server and keeps each raw reply, byte for byte.

Usage: resp.py (--unix PATH | --tcp HOST PORT) [--password-file FILE] OUT COMMAND...
Each COMMAND is one string, split on spaces; its reply is written to OUT/<n>-<command>.resp.
`AUTH` is sent first where a password file is given, and its reply kept like any other.
"""

import argparse
import pathlib
import socket
import sys

DEADLINE_SECONDS = 10


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
    target.add_argument("--tcp", nargs=2, metavar=("HOST", "PORT"))
    parser.add_argument("--password-file")
    parser.add_argument("out")
    parser.add_argument("commands", nargs="+")
    arguments = parser.parse_args()

    if arguments.unix:
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(DEADLINE_SECONDS)
        sock.connect(arguments.unix)
    else:
        host, port = arguments.tcp
        sock = socket.create_connection((host, int(port)), timeout=DEADLINE_SECONDS)

    out = pathlib.Path(arguments.out)
    out.mkdir(parents=True, exist_ok=True)
    reader = Reader(sock)
    commands = [command.split(" ") for command in arguments.commands]
    if arguments.password_file:
        # One word, the default account's password, or two, an account and its password.
        credential = pathlib.Path(arguments.password_file).read_text().split()
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
