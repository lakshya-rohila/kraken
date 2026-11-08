#!/usr/bin/env python3
"""Run a shell in a pty with Kraken's integration env/args (mirrors shell.rs `plan`)
and check the OSC marks. Usage: live_shell_test.py <repo> <shell-path> [scenario]"""
import os, pty, sys, time, select, shutil, tempfile, re

repo, shell = sys.argv[1], sys.argv[2]
scenario = sys.argv[3] if len(sys.argv) > 3 else "plain"
src = os.path.join(repo, "crates/kraken-term/shell")
name = os.path.basename(shell)
d = tempfile.mkdtemp(prefix="kraken-live-")
home = os.path.join(d, "home"); os.makedirs(home)
integ = os.path.join(d, "shell-integration")
def install(rel, f):
    p = os.path.join(integ, rel); os.makedirs(os.path.dirname(p), exist_ok=True); shutil.copy(os.path.join(src, f), p)

env = {"HOME": home, "PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "TERM": "xterm-256color", "LANG": "en_US.UTF-8", "USER": os.environ.get("USER", "x")}
args = [shell, "-l"]
if name == "bash":
    install("bash/kraken.bash", "kraken.bash")
    env["KRAKEN_BASH_INJECT"] = "login"
    args = [shell, "--rcfile", os.path.join(integ, "bash/kraken.bash")]
    prof = 'echo PROFILE_LOADED\n'
    if scenario == "hostile":
        prof += "PROMPT_COMMAND='echo USER_PC;'\ntrap 'USER_DEBUG_RAN=1' DEBUG\n"
        prof += ". %s\n" % os.path.join(integ, "bash/kraken.bash")  # sourced again later by --rcfile: must not double-install
    open(os.path.join(home, ".bash_profile"), "w").write(prof)
elif name in ("fish", "nu"):
    install("data/fish/vendor_conf.d/kraken.fish", "kraken.fish")
    install("data/nushell/vendor/autoload/kraken.nu", "kraken.nu")
    data = os.path.join(integ, "data")
    env["XDG_DATA_DIRS"] = data + ":/usr/local/share:/usr/share"; env["KRAKEN_SHELL_DATA_DIR"] = data
elif name in ("pwsh", "powershell"):
    install("pwsh/kraken.ps1", "kraken.ps1")
    args = [shell, "-Login", "-NoExit", "-Command", ". '%s'" % os.path.join(integ, "pwsh/kraken.ps1").replace("'", "''")]

inputs = ["cd /tmp", "false", "", "echo XDG=[$XDG_DATA_DIRS] ENV_INJECT=[$KRAKEN_BASH_INJECT]", "exit"]
if name == "bash" and scenario == "hostile":
    inputs.insert(3, 'echo DEBUG_CHAIN=$USER_DEBUG_RAN')
pid, fd = pty.fork()
if pid == 0:
    os.chdir(home); os.execve(shell, args, env)
out = b""
def pump(t):
    global out
    end = time.time() + t
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.1)
        if r:
            try: b = os.read(fd, 65536)
            except OSError: return
            if not b: return
            out += b
pump(3)
for line in inputs:
    os.write(fd, line.encode() + b"\r"); pump(1.5)
os.waitpid(pid, 0)
shutil.rmtree(d)
marks = re.findall(rb"\x1b\](133;[A-D][^\x07\x1b]*|7;[^\x07\x1b]*)", out)
print("marks:", [m.decode() for m in marks])
ok = True
def check(cond, msg):
    global ok
    print(("PASS " if cond else "FAIL ") + msg); ok &= bool(cond)
s = [m.decode() for m in marks]
check(any(m.startswith("7;file://") and m.endswith("/tmp") for m in s) or any(m.startswith("7;file://") and m.endswith("/private/tmp") for m in s), "OSC 7 with /tmp")
check("133;D;1" in s, "OSC 133;D;1 after `false`")
check("133;C" in s or any(m.startswith("133;C") for m in s), "OSC 133;C")
i = s.index("133;D;1") if "133;D;1" in s else -1
check(i > 0 and s[i-1].startswith("133;C"), "C directly precedes D;1")
# Empty Enter after `false`: next marks are 7, A then (no C/D) 7, A
if i >= 0:
    nxt = [m for m in s[i+1:] if m.startswith("133;")][:3]
    check(nxt[:2] == ["133;A", "133;A"] , "empty line gives no C/D: %s" % nxt)
txt = out.decode(errors="replace")
if name == "bash":
    check("PROFILE_LOADED" in txt, "login profile sourced")
    check("ENV_INJECT=[]" in txt, "KRAKEN_BASH_INJECT cleared")
    check(s.count("133;A") == len([m for m in s if m == "133;A"]) and txt.count("\x1b]133;D;1") == 1, "no double install (single D;1)")
    if scenario == "hostile":
        check("USER_PC" in txt, "user's PROMPT_COMMAND still runs")
        check("DEBUG_CHAIN=1" in txt, "user's DEBUG trap still runs")
if name in ("fish", "nu"):
    check("XDG=[]" in txt, "XDG_DATA_DIRS restored (unset)")
print("OK" if ok else "FAILED"); sys.exit(0 if ok else 1)
