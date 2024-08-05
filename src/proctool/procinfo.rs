use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Read},
};

use anyhow::Result;
use nix::{fcntl, sys, unistd};

pub fn print_process_tree(mut pid: i32) -> Result<()> {
    let mut stack = Vec::new();
    while pid != 0 {
        let info = get_process_info(pid)?;
        pid = info.ppid;
        stack.push(info);
    }

    let mut indent = 0;
    for info in stack.iter().rev() {
        let leader = if info.pid == info.pgid {
            ", leader"
        } else {
            ""
        };

        let sid = unistd::getsid(Some(unistd::Pid::from_raw(info.pid)))?;
        println!(
            "{:indent$}{}  {} (group: {}{}, session: {}, tty: {})",
            "",
            info.pid,
            info.name,
            info.pgid,
            leader,
            sid,
            info.tty.as_ref().unwrap_or(&"<unknown>".to_string()),
            indent = indent
        );
        indent += 2;
    }

    Ok(())
}

pub fn print_process_groups() -> Result<()> {
    for (gid, processes) in get_all_groups()? {
        let n = processes.len();
        println!("{}: {} process{}", gid, n, if n == 1 { "" } else { "es" });
    }

    Ok(())
}

pub fn get_all_groups() -> Result<HashMap<i32, Vec<ProcessInfo>>> {
    let mut groups: HashMap<i32, Vec<ProcessInfo>> = HashMap::new();
    for info in get_all_process_info()? {
        if let Some(v) = groups.get_mut(&info.pid) {
            v.push(info);
        } else {
            groups.insert(info.pid, vec![info]);
        }
    }
    Ok(groups)
}

pub fn print_sessions() -> Result<()> {
    let mut sessions: HashMap<i32, Vec<i32>> = HashMap::new();
    for (process_leader_id, _) in get_all_groups()? {
        if let Ok(sid) = unistd::getsid(Some(unistd::Pid::from_raw(process_leader_id))) {
            if let Some(v) = sessions.get_mut(&sid.as_raw()) {
                v.push(process_leader_id);
            } else {
                sessions.insert(sid.as_raw(), vec![process_leader_id]);
            }
        }
    }

    for (sid, groups) in sessions {
        // TODO: print controlling terminal and foreground process group
        println!("{}", sid);
        for gid in groups {
            println!("  {}", gid);
        }
    }

    Ok(())
}

pub fn list_processes() -> Result<()> {
    let uid = unistd::getuid();
    for info in get_all_process_info()? {
        if info.uid != uid.as_raw() {
            continue;
        }

        println!("{}  {}", info.pid, info.name);
    }
    Ok(())
}

pub fn list_terminals() -> Result<()> {
    for entry_result in fs::read_dir("/dev/pts")? {
        let entry = entry_result?;
        let fd = fcntl::open(
            &entry.path(),
            fcntl::OFlag::O_RDONLY,
            sys::stat::Mode::empty(),
        )?;

        // TODO: this only works for finding your own controlling terminal, not another process's
        let pgrp = unsafe { libc::tcgetpgrp(fd) };
        let sid = unsafe { libc::tcgetsid(fd) };

        println!("{}", entry.path().display());
        println!("  session: {}", sid);
        println!("  fg grp:  {}", pgrp);
        unistd::close(fd)?;
    }
    Ok(())
}

fn get_all_process_info() -> Result<Vec<ProcessInfo>> {
    let mut r = Vec::new();
    for entry_result in fs::read_dir("/proc")? {
        let entry = entry_result?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() {
            continue;
        }

        let name_result = entry.file_name().into_string();
        if name_result.is_err() {
            continue;
        }

        let name = name_result.unwrap();
        let pid_result = name.parse::<i32>();
        if !pid_result.is_ok() {
            continue;
        }
        let pid = pid_result.unwrap();

        let info = get_process_info(pid)?;
        r.push(info);
    }
    Ok(r)
}

pub struct ProcessInfo {
    name: String,
    pid: i32,
    ppid: i32,
    pgid: i32,
    uid: u32,
    tty: Option<String>,
}

pub fn get_process_info(pid: i32) -> Result<ProcessInfo> {
    let f = fs::File::open(format!("/proc/{}/status", pid))?;
    let reader = BufReader::new(f);

    let mut attributes = HashMap::new();
    for line_result in reader.lines() {
        let line = line_result.unwrap();
        let (left, right) = line.split_once(":\t").unwrap();
        attributes.insert(left.to_string(), right.to_string());
    }

    let stat_fields = read_proc_stat(pid)?;
    // based on:
    //   https://github.com/dalance/procs/blob/0c789712207444654b9fbf75b3ed7ad3d5114344/src/columns/tty.rs#L32
    //   https://github.com/eminence/procfs/blob/784dd2c30df4c6d581b24d6ff74c81bb4a50a069/procfs-core/src/process/stat.rs#L372
    let tty_major = (stat_fields.tty_nr & 0xfff00) >> 8;
    let tty_minor = (stat_fields.tty_nr & 0x000ff) | ((stat_fields.tty_nr >> 12) & 0xfff00);
    let tty = if tty_major == 136 {
        Some(format!("/dev/pts/{}", tty_minor))
    } else {
        None
    };

    let name = attributes.get("Name").unwrap().clone();
    let ppid = attributes.get("PPid").unwrap().parse::<i32>().unwrap();
    let pgid = attributes.get("NSpgid").unwrap().parse::<i32>().unwrap();
    let uid = attributes
        .get("Uid")
        .unwrap()
        .split_ascii_whitespace()
        .next()
        .unwrap()
        .parse::<u32>()
        .unwrap();
    Ok(ProcessInfo {
        name,
        pid,
        ppid,
        pgid,
        uid,
        tty,
    })
}

fn read_proc_stat(pid: i32) -> Result<StatFields> {
    let mut f = fs::File::open(format!("/proc/{}/stat", pid))?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;

    let parts: Vec<&str> = s.split_ascii_whitespace().collect();
    let mut i = 1;
    while i < parts.len() && !parts[i].ends_with(")") {
        i += 1;
    }
    let offset = i - 1;

    Ok(StatFields {
        // tty_nr is field 7, but it's 1-indexed
        tty_nr: parts[6 - offset].parse::<i32>()?,
    })
}

struct StatFields {
    tty_nr: i32,
}
