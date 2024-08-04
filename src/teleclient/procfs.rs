use core::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct MemoryMap {
    pub base_address: u64,
    pub size: u64,
    pub label: String,
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub private: bool,
    pub data: Vec<u8>,
}

pub fn get_command_line(pid: i32) -> Result<Vec<Vec<u8>>> {
    let path = format!("/proc/{}/cmdline", pid);
    let mut file = File::open(&path)?;

    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;

    let mut start = 0;
    let mut i = 0;
    let mut args = Vec::new();

    while i < buf.len() {
        if buf[i] == 0 {
            args.push(Vec::from(&buf[start..i+1]));
            start = i + 1;
        }
        i += 1;
    }

    Ok(args)
}

pub fn read_memory_maps(pid: i32) -> Result<Vec<MemoryMap>> {
    let path = format!("/proc/{}/maps", pid);
    let file = File::open(&path)?;
    let reader = BufReader::new(file);

    let mut r = Vec::new();
    for line_result in reader.lines() {
        let line = line_result?;
        r.push(parse_map_line(&line)?);
    }
    Ok(r)
}

fn parse_map_line(line: &str) -> Result<MemoryMap> {
    let parts: Vec<&str> = line.splitn(6, char::is_whitespace).collect();
    let byte_range = parts[0];
    let permissions = parts[1];
    let label = parts[5];

    let (base_address, size) = parse_byte_range(byte_range)?;
    let (readable, writable, executable, private) = parse_permissions(permissions)?;

    Ok(MemoryMap {
        base_address,
        size,
        label: label.trim().to_string(),
        readable,
        writable,
        executable,
        private,
        data: Vec::new(),
    })
}

/// returns (base_address, size)
fn parse_byte_range(byte_range: &str) -> Result<(u64, u64)> {
    let parts: Vec<&str> = byte_range.splitn(2, "-").collect();
    let start_address = u64::from_str_radix(parts[0], 16)
        .map_err(|e| anyhow!("could not parse byte range start: {}", e))?;
    let end_address = u64::from_str_radix(parts[1], 16)
        .map_err(|e| anyhow!("could not parse byte range end: {}", e))?;
    Ok((start_address, end_address - start_address))
}

/// returns (readable, writable, executable)
fn parse_permissions(permissions: &str) -> Result<(bool, bool, bool, bool)> {
    let chars: Vec<char> = permissions.chars().collect();
    if chars.len() != 4 {
        return Err(anyhow!(
            "expected permissions field to be exactly 4 chars long, got {}",
            permissions.len()
        ));
    }

    let readable = chars[0] == 'r';
    let writable = chars[1] == 'w';
    let executable = chars[2] == 'x';
    let private = chars[3] == 'p';

    Ok((readable, writable, executable, private))
}

impl fmt::Display for MemoryMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "mapping from {:#x} to {:#x} (size={}) ",
            self.base_address,
            self.base_address + self.size,
            self.size
        )?;
        write!(f, "{}", if self.readable { "r" } else { "-" })?;
        write!(f, "{}", if self.writable { "w" } else { "-" })?;
        write!(f, "{}", if self.executable { "x" } else { "-" })?;
        write!(f, "{}", if self.private { "p" } else { "-" })?;

        if !self.label.is_empty() {
            write!(f, " ({})", self.label)?;
        }

        std::fmt::Result::Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_map_line;

    #[test]
    fn test_parse_memory_map_line() {
        let memory_map = parse_map_line("e4ba32e70000-e4ba3300a000 r-xp 00000000 fc:00 298576                     /usr/lib/aarch64-linux-gnu/libc.so.6\n").unwrap();
        assert_eq!(memory_map.base_address, 0xe4ba32e70000);
        assert_eq!(memory_map.size, 1679360);
        assert!(memory_map.readable);
        assert!(!memory_map.writable);
        assert!(memory_map.executable);
        assert!(memory_map.private);
        assert_eq!(memory_map.label, "/usr/lib/aarch64-linux-gnu/libc.so.6");
    }
}
