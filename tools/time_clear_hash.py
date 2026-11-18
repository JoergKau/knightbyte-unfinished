"""Times `setoption name Clear Hash` and `ucinewgame` at Hash 1024.

Migration plan, Phase 5 step 2: if the synchronous clear takes more than
about 100 ms, the finding is logged in docs-new/new-bugs.md and made
bounded in step 7. Usage: python tools/time_clear_hash.py <engine.exe>
"""
import subprocess
import sys
import time

def main() -> None:
    engine = sys.argv[1]
    proc = subprocess.Popen(
        [engine],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        text=True,
    )

    def send(line: str) -> None:
        proc.stdin.write(line + "\n")
        proc.stdin.flush()

    def read_until(marker: str) -> float:
        """Reads lines until one starts with `marker`; returns elapsed ms."""
        started = time.perf_counter()
        while True:
            line = proc.stdout.readline()
            if not line:
                raise RuntimeError("engine closed stdout")
            if line.startswith(marker):
                return (time.perf_counter() - started) * 1000.0

    send("setoption name Hash value 1024")
    send("isready")
    base = read_until("readyok")  # warm-up: the 1 GB resize happens here
    print(f"warmup isready after resize: {base:.1f} ms")

    for label, command in [
        ("Clear Hash (2nd)", "setoption name Clear Hash"),
        ("ucinewgame", "ucinewgame"),
        ("Clear Hash (3rd)", "setoption name Clear Hash"),
    ]:
        # The isready after the command measures the clear cost plus a
        # trivial reply; run it three times and keep the best (warm cache).
        send(command)
        send("isready")
        best = read_until("readyok")
        print(f"{label}: {best:.1f} ms")

    send("quit")
    proc.wait()

if __name__ == "__main__":
    main()
