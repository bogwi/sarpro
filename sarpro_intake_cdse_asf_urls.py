import os
import subprocess

# Change these to match your environment
REMOTE_CACHE = "/path/to/sarpro_cache"
OUT = "/path/to/sarpro_output/urls_ASF_CDSE"
SARPRO = "/path/to/sarpro/target/release/sarpro"
URLS_FILE = "/path/to/urls.txt"

os.makedirs(REMOTE_CACHE, exist_ok=True)
os.makedirs(OUT, exist_ok=True)

def build_cmd(url):
    url = url.strip()
    if not url or url.startswith("#"):
        return None
    url_no_proto = url.replace("https://", "")
    domain = url_no_proto.split("/")[0]
    output_dir = os.path.join(OUT, domain)
    os.makedirs(output_dir, exist_ok=True)
    print(f"Processing: {url} -> {output_dir}")
    return [
        SARPRO,
        "--safe-zip-url", url,
        "--remote-cache-dir", REMOTE_CACHE,
        "--output", output_dir,
        "--format", "jpeg",
        "--polarization", "multiband",
        "--size", "2048",
        "--autoscale", "tamed",
        "--target-crs", "auto",
        "--log"
    ]

with open(URLS_FILE) as f:
    urls = [line for line in f if line.strip() and not line.strip().startswith("#")]

procs = []
try:
    for url in urls:
        cmd = build_cmd(url)
        if cmd:
            # Start all jobs at once
            procs.append(subprocess.Popen(cmd))

    # Wait for all to finish 
    for p in procs:
        p.wait()
except KeyboardInterrupt:
    print("\nInterrupted! Terminating all running subprocesses...")
    for p in procs:
        if p.poll() is None:  # still running
            p.terminate()
    for p in procs:
        if p.poll() is None:
            p.wait()
    print("All subprocesses terminated.")