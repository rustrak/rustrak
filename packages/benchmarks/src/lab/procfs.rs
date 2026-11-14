//! Memory, I/O and CPU of one Linux process, read from `/proc/<pid>`, plus the
//! container's cgroup counter (which includes page cache, so it is reported
//! only as a labelled extra).

use serde::Serialize;

/// One reading of the server process.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Sample {
    /// Seconds since the run started.
    pub t: f64,
    /// `smaps_rollup` `Anonymous`: heap, stacks, anonymous mmaps. The number
    /// that answers "how much RAM does the server use".
    pub anon: u64,
    /// `status` `RssFile`: mapped file pages (the binary, mmapped DB pages).
    pub file: u64,
    /// `status` `VmHWM`: peak RSS (anon + file) since start.
    pub hwm: u64,
    pub threads: u64,
    /// `io` `write_bytes`: bytes this process caused to reach storage.
    pub write_bytes: u64,
    /// `stat` utime + stime, in clock ticks.
    pub cpu_ticks: u64,
    /// The container cgroup's `memory.current`, page cache included, and
    /// everything else in the container (load generator) too.
    pub cgroup_incl_cache: u64,
    /// The container cgroup's `io.stat` `wbytes`: what reached the disk.
    /// `write_bytes` counts a whole page-cache folio per write into it, so
    /// a 4 KiB write into a large folio (a file written or read in big
    /// chunks, as the seeded database is) counts up to megabytes.
    pub disk_write_bytes: u64,
}

/// Sum of `wbytes=` over the devices of a cgroup `io.stat`.
fn io_stat_wbytes(stat: &str) -> u64 {
    stat.split_whitespace()
        .filter_map(|kv| kv.strip_prefix("wbytes=")?.parse::<u64>().ok())
        .sum()
}

/// Value of `key:` in a `/proc` key/value file, in bytes when it ends in kB.
fn field(text: &str, key: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let rest = line.strip_prefix(key)?.strip_prefix(':')?;
        let mut parts = rest.split_whitespace();
        let value: u64 = parts.next()?.parse().ok()?;
        Some(if parts.next() == Some("kB") {
            value * 1024
        } else {
            value
        })
    })
}

/// utime + stime from `/proc/<pid>/stat` (fields 14 and 15, after the
/// parenthesised command name, which may itself contain spaces).
fn cpu_ticks(stat: &str) -> Option<u64> {
    let after = &stat[stat.rfind(')')? + 2..];
    let fields: Vec<&str> = after.split_whitespace().collect();
    Some(fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?)
}

pub fn sample(pid: u32, t: f64) -> Option<Sample> {
    let read = |name: &str| std::fs::read_to_string(format!("/proc/{pid}/{name}")).ok();
    let rollup = read("smaps_rollup")?;
    let status = read("status")?;
    let io = read("io").unwrap_or_default();
    let stat = read("stat").unwrap_or_default();
    let cgroup = std::fs::read_to_string("/sys/fs/cgroup/memory.current")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    Some(Sample {
        t,
        anon: field(&rollup, "Anonymous")?,
        file: field(&status, "RssFile").unwrap_or(0),
        hwm: field(&status, "VmHWM").unwrap_or(0),
        threads: field(&status, "Threads").unwrap_or(0),
        write_bytes: field(&io, "write_bytes").unwrap_or(0),
        cpu_ticks: cpu_ticks(&stat).unwrap_or(0),
        cgroup_incl_cache: cgroup,
        disk_write_bytes: std::fs::read_to_string("/sys/fs/cgroup/io.stat")
            .map(|s| io_stat_wbytes(&s))
            .unwrap_or(0),
    })
}

/// Clock ticks per second (`getconf CLK_TCK`); 100 on every Linux we run on.
pub const TICKS_PER_SEC: f64 = 100.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_kb_fields_as_bytes_and_plain_counts_as_is() {
        let status = "Name:\trustrak\nVmHWM:\t   51200 kB\nRssFile:\t  1024 kB\nThreads:\t17\n";
        assert_eq!(field(status, "VmHWM"), Some(51200 * 1024));
        assert_eq!(field(status, "RssFile"), Some(1024 * 1024));
        assert_eq!(field(status, "Threads"), Some(17));
        assert_eq!(field(status, "Missing"), None);
        // A key that is a prefix of another is not confused with it.
        assert_eq!(field("RssFileX: 5 kB\n", "RssFile"), None);
    }

    #[test]
    fn reads_anonymous_from_smaps_rollup() {
        let rollup =
            "55d0-7ffd ---p 00000000 00:00 0 [rollup]\nRss:  30000 kB\nAnonymous:  21000 kB\n";
        assert_eq!(field(rollup, "Anonymous"), Some(21000 * 1024));
    }

    #[test]
    fn sums_written_bytes_over_every_device_in_io_stat() {
        let stat = "254:0 rbytes=10 wbytes=4096 rios=1 wios=1 dbytes=0 dios=0\n\
                    254:16 rbytes=0 wbytes=8192 rios=0 wios=2 dbytes=0 dios=0\n";
        assert_eq!(io_stat_wbytes(stat), 4096 + 8192);
        assert_eq!(io_stat_wbytes(""), 0);
    }

    #[test]
    fn cpu_ticks_skip_a_command_name_with_spaces() {
        let stat = "42 (tokio rt) S 1 42 42 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 9 0";
        assert_eq!(cpu_ticks(stat), Some(300));
    }
}
