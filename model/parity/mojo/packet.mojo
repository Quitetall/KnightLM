"""Mojo 1.0 protocol slice. CPython handles JSON; counter checks run in Mojo."""
from std.python import Python
from std.sys import argv


def main() raises:
    var args = argv()
    if len(args) != 3:
        raise Error("usage: packet.mojo PYTHON_PORT_DIRECTORY PACKET_JSON")
    var sys = Python.import_module("sys")
    sys.path.insert(0, args[1])
    var protocol = Python.import_module("knightlm_contract")
    var packet = protocol.load_packet(args[2])
    var builtins = Python.import_module("builtins")
    var fields = packet["fen"].split()
    var halfmove = Int(py=builtins.int(fields[4]))
    var fullmove = Int(py=builtins.int(fields[5]))
    if halfmove < 0 or fullmove < 1:
        raise Error("invalid native counter")
    var facts = packet["facts"]
    var terminal = Bool(py=builtins.bool(facts["terminal_reason"]))
    var checkmate = Bool(py=facts["checkmate"])
    if Bool(py=facts["fifty_move_claim_now"]) != (halfmove >= 100 and not terminal):
        raise Error("50-move claim disagrees with counter")
    if Bool(py=facts["seventy_five_move_automatic"]) != (halfmove >= 150 and not checkmate):
        raise Error("75-move status disagrees with counter")
    print(protocol.canonical(packet))
