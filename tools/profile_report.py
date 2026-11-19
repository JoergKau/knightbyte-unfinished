#!/usr/bin/env python3
"""Aggregate a samply profile (saved with --save-only) by function using dbghelp.

Usage: python tools/profile_report.py PROFILE.json.gz EXE [--top 40]

Windows only. Leaf frames are resolved through the PDB next to EXE with
dbghelp, including inlined callees, and reported as self time per function
(inlined functions are attributed to themselves, and `outer` shows the
real machine function containing them).
"""
import argparse, ctypes, gzip, json, collections
from ctypes import wintypes

ap = argparse.ArgumentParser()
ap.add_argument("profile"); ap.add_argument("exe"); ap.add_argument("--top", type=int, default=40)
ap.add_argument("--main-only", action="store_true", help="only the busiest thread")
args = ap.parse_args()

p = json.load(gzip.open(args.profile))
libs = p["libs"]
threads = [t for t in p["threads"] if t["samples"]["length"] > 50]

import os
_sdk = r"C:\Program Files (x86)\Windows Kits\10\Debuggers\x64\dbghelp.dll"
dbg = ctypes.WinDLL(_sdk if os.path.exists(_sdk) else "dbghelp.dll", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
kernel32.GetCurrentProcess.restype = wintypes.HANDLE
proc = kernel32.GetCurrentProcess()
H = wintypes.HANDLE; U64 = ctypes.c_uint64
dbg.SymInitialize.argtypes = [H, wintypes.LPCSTR, wintypes.BOOL]
dbg.SymFromAddr.argtypes = [H, U64, ctypes.POINTER(U64), ctypes.c_void_p]
dbg.SymFromInlineContext.argtypes = [H, U64, wintypes.ULONG, ctypes.POINTER(U64), ctypes.c_void_p]
dbg.SymAddrIncludeInlineTrace.argtypes = [H, U64]
dbg.SymQueryInlineTrace.argtypes = [H, U64, wintypes.DWORD, U64, U64, ctypes.POINTER(wintypes.DWORD), ctypes.POINTER(wintypes.DWORD)]
dbg.SymSetOptions(0x2 | 0x4)  # UNDNAME | DEFERRED_LOADS
dbg.SymInitialize(proc, None, True)
BASE = 0x140000000
dbg.SymLoadModuleExW.restype = ctypes.c_uint64
dbg.SymLoadModuleExW.argtypes = [wintypes.HANDLE, wintypes.HANDLE, wintypes.LPCWSTR, wintypes.LPCWSTR, ctypes.c_uint64, wintypes.DWORD, ctypes.c_void_p, wintypes.DWORD]
base = dbg.SymLoadModuleExW(proc, None, args.exe, None, BASE, 0, None, 0)
assert base, f"cannot load module (error {ctypes.get_last_error()})"

class SYMBOL_INFO(ctypes.Structure):
    _fields_ = [("SizeOfStruct", wintypes.ULONG), ("TypeIndex", wintypes.ULONG), ("Reserved", ctypes.c_uint64 * 2),
                ("Index", wintypes.ULONG), ("Size", wintypes.ULONG), ("ModBase", ctypes.c_uint64), ("Flags", wintypes.ULONG),
                ("Value", ctypes.c_uint64), ("Address", ctypes.c_uint64), ("Register", wintypes.ULONG), ("Scope", wintypes.ULONG),
                ("Tag", wintypes.ULONG), ("NameLen", wintypes.ULONG), ("MaxNameLen", wintypes.ULONG), ("Name", ctypes.c_char * 1024)]

def sym(addr, inline_ctx=None):
    si = SYMBOL_INFO(); si.SizeOfStruct = 88; si.MaxNameLen = 1024
    disp = ctypes.c_uint64(0)
    if inline_ctx is None:
        ok = dbg.SymFromAddr(proc, ctypes.c_uint64(addr), ctypes.byref(disp), ctypes.byref(si))
    else:
        ok = dbg.SymFromInlineContext(proc, ctypes.c_uint64(addr), wintypes.ULONG(inline_ctx), ctypes.byref(disp), ctypes.byref(si))
    return si.Name.decode(errors="replace") if ok else None

CB = ctypes.WINFUNCTYPE(wintypes.BOOL, ctypes.POINTER(SYMBOL_INFO), wintypes.ULONG, ctypes.c_void_p)
table = []
def _collect(p, size, ctx):
    s = p.contents
    table.append((s.Address - BASE, s.Size, s.Name.decode(errors="replace")))
    return True
dbg.SymEnumSymbols.argtypes = [H, U64, wintypes.LPCSTR, CB, ctypes.c_void_p]
_cb = CB(_collect)
dbg.SymEnumSymbols(proc, BASE, b"*", _cb, None)
table.sort()
import bisect
starts = [t[0] for t in table]
cache = {}
def resolve(addr):
    rva = addr - BASE
    i = bisect.bisect_right(starts, rva) - 1
    name = "?"
    if i >= 0:
        start, size, n = table[i]
        if rva < start + max(size, 1) or size == 0:
            name = n
    return (name, name)
# Which lib index is the exe?
exe_lib = [i for i, l in enumerate(libs) if l["name"].lower().startswith("knightbyte")][0]
if args.main_only:
    threads = [max(threads, key=lambda t: t["samples"]["length"])]
self_t = collections.Counter(); total = 0
for thread in threads:
    frames, funcs, stacks, res = thread["frameTable"], thread["funcTable"], thread["stackTable"], thread["resourceTable"]
    strings = thread["stringArray"]
    for s in thread["samples"]["stack"]:
        if s is None: continue
        f = stacks["frame"][s]
        addr = frames["address"][f]
        fn = frames["func"][f]
        r = funcs["resource"][fn]
        total += 1
        if r is not None and r >= 0 and res["lib"][r] == exe_lib and addr >= 0:
            name = resolve(BASE + addr)[0]
        else:
            lib = libs[res["lib"][r]]["name"] if r is not None and r >= 0 else "?"
            name = f"({lib}) " + strings[funcs["name"][fn]]
        self_t[name] += 1

print(f"{len(threads)} thread(s), total samples {total}")
for name, n in self_t.most_common(args.top):
    print(f"{100*n/total:6.2f}%  {n:6d}  {name[:110]}")