#!/usr/bin/env python3
"""List the DLLs and functions a Windows PE executable imports.

Used for the Windows 10 compatibility check: every imported function must
exist on Windows 10, so a function that only newer Windows versions export
shows up here by name.

Usage:
    python tools/pe_imports.py target/release/knightbyte.exe
"""

import struct
import sys


def read_cstring(data, offset):
    end = data.index(b"\0", offset)
    return data[offset:end].decode("ascii", "replace")


def main():
    data = open(sys.argv[1], "rb").read()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe : pe + 4] != b"PE\0\0":
        raise SystemExit("not a PE file")
    sections_count = struct.unpack_from("<H", data, pe + 6)[0]
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    magic = struct.unpack_from("<H", data, optional)[0]
    plus = magic == 0x20B
    directories = optional + (112 if plus else 96)
    import_rva = struct.unpack_from("<I", data, directories + 8)[0]
    sections = optional + optional_size

    def rva_to_offset(rva):
        for index in range(sections_count):
            base = sections + index * 40
            virtual_size, virtual_address, raw_size, raw_pointer = struct.unpack_from(
                "<IIII", data, base + 8
            )
            if virtual_address <= rva < virtual_address + max(virtual_size, raw_size):
                return rva - virtual_address + raw_pointer
        raise ValueError(f"RVA {rva:#x} is outside every section")

    descriptor = rva_to_offset(import_rva)
    while True:
        original_first_thunk, _, _, name_rva, first_thunk = struct.unpack_from(
            "<IIIII", data, descriptor
        )
        if name_rva == 0:
            break
        dll = read_cstring(data, rva_to_offset(name_rva))
        thunk = rva_to_offset(original_first_thunk or first_thunk)
        names = []
        step = 8 if plus else 4
        while True:
            value = int.from_bytes(data[thunk : thunk + step], "little")
            if value == 0:
                break
            if value >> (step * 8 - 1):
                names.append(f"#{value & 0xFFFF}")
            else:
                names.append(read_cstring(data, rva_to_offset(value & 0x7FFFFFFF) + 2))
            thunk += step
        print(f"{dll} ({len(names)})")
        for name in sorted(names):
            print(f"    {name}")
        descriptor += 20


if __name__ == "__main__":
    main()
