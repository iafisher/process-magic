import fcntl
import glob
import os
import termios
from collections import defaultdict
from pathlib import Path


"""
- Every process has a PID.
- Every process belongs to a group.
- Groups have leaders.
- Groups don't have IDs, but you can refer to them by the ID of the group leader.
- Groups can belong to sessions.
- Sessions have leaders.
- Sessions don't have IDs, but you can refer to them by the PID of the session leader.
"""


def get_parent_pid(pid):
    return int(_read_proc_status(pid)["PPid"])


def get_group_id(pid):
    return int(_read_proc_status(pid)["NSpgid"])


def is_group_leader(pid):
    return pid == get_group_id(pid)


def is_session_leader(pid):
    return pid == get_session_id(pid)


# sessions don't have IDs but we can refer to them by the ID of the session leader
def get_session_id(pid):
    return os.getsid(pid)


def get_controlling_terminal(pid):
    stat = Path(f"/proc/{pid}/stat").read_text()
    index = stat.find(")")
    if index == -1:
        return None

    # manpage: proc_pid_stat(5)
    fields = stat[index+1:].split()
    tty_nr = int(fields[4])
    major = (tty_nr & 0xfff00) >> 8
    minor = (tty_nr & 0x000ff) | ((tty_nr >> 12) & 0xfff00)
    if major == 136:
        return f"/dev/pts/{minor}"
    else:
        return None


def get_session_id_for_terminal(term):
    # TODO: is there a better way to do this?
    for pid in list_pids():
        if get_controlling_terminal(pid) == term:
            return get_session_id(pid)

    return None


def get_foreground_group_for_terminal(term):
    with open(term, 'rb') as tty:
        return os.tcgetpgrp(tty.fileno())
        # pgid = fcntl.ioctl(tty, termios.TIOCGPGRP, ' ' * 4)
        # pgid = int.from_bytes(pgid, byteorder='little')
        # return pgid


def list_pids():
    for fpath in glob.iglob("/proc/*/status"):
        fields = _read_proc_status_file(fpath)
        yield int(fields["Pid"])


def _read_proc_status(pid):
    return _read_proc_status_file(f"/proc/{pid}/status")


def _read_proc_status_file(fpath):
    fields = defaultdict(list)
    with open(fpath, "r") as f:
        for line in f:
            left, right = line.split(":\t", maxsplit=1)
            fields[left] = right.rstrip()

    return fields


pid = 2660891
print("parent  ", get_parent_pid(pid))
print("group   ", get_group_id(pid))
print("session ", get_session_id(pid))
print("terminal", get_controlling_terminal(pid))
print()
print("is group leader  ", is_group_leader(pid))
print("is session leader", is_session_leader(pid))

# print("/dev/pts/6:")
# session_id = get_session_id_for_terminal("/dev/pts/6")
# print("  session:", session_id)
# print("  fg grp: ", get_foreground_group_for_terminal("/dev/pts/6"))
