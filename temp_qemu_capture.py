import pathlib
import subprocess

workspace = pathlib.Path(r"c:\Users\jungamer64\Desktop\OS")
bin_path = workspace / "target" / "x86_64-blog_os" / "debug" / "bootimage-tiny_os.bin"
cmd = [
    "qemu-system-x86_64",
    "-drive", f"format=raw,file={bin_path}",
    "-serial", "stdio",
    "-display", "none",
    "-m", "128M",
    "-no-reboot",
    "-no-shutdown",
]
print("Running:", " ".join(cmd))
proc = subprocess.run(cmd, cwd=workspace, capture_output=True, text=True)
print("QEMU exited with", proc.returncode)
(workspace / "serial.log").write_text(proc.stdout + "\nSTDERR:\n" + proc.stderr)
