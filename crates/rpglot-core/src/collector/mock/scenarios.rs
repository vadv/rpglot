//! Pre-built mock filesystem scenarios for testing.
//!
//! These scenarios provide realistic `/proc` filesystem states
//! for testing various system conditions.

use super::filesystem::MockFs;

#[allow(dead_code)]
impl MockFs {
    /// Creates a typical system with a few processes.
    ///
    /// Includes: init (PID 1), bash shell, and a simple daemon.
    pub fn typical_system() -> Self {
        let mut fs = Self::new();

        // /etc/passwd for user name resolution
        fs.add_file(
            "/etc/passwd",
            "\
root:x:0:0:root:/root:/bin/bash
daemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin
bin:x:2:2:bin:/bin:/usr/sbin/nologin
sys:x:3:3:sys:/dev:/usr/sbin/nologin
nobody:x:65534:65534:nobody:/nonexistent:/usr/sbin/nologin
user:x:1000:1000:User:/home/user:/bin/bash
",
        );

        // System-wide files
        fs.add_file("/proc/uptime", "12345.67 98765.43\n");
        fs.add_file("/proc/loadavg", "0.15 0.10 0.05 1/150 1234\n");
        fs.add_file(
            "/proc/meminfo",
            "\
MemTotal:       16384000 kB
MemFree:         8192000 kB
MemAvailable:   12000000 kB
Buffers:          512000 kB
Cached:          2048000 kB
SwapCached:            0 kB
Active:          4096000 kB
Inactive:        2048000 kB
SwapTotal:       4096000 kB
SwapFree:        4096000 kB
Dirty:              1024 kB
Writeback:             0 kB
Slab:             512000 kB
SReclaimable:     256000 kB
",
        );
        fs.add_file(
            "/proc/stat",
            "\
cpu  10000 500 3000 80000 1000 200 100 0 0 0
cpu0 2500 125 750 20000 250 50 25 0 0 0
cpu1 2500 125 750 20000 250 50 25 0 0 0
cpu2 2500 125 750 20000 250 50 25 0 0 0
cpu3 2500 125 750 20000 250 50 25 0 0 0
intr 1000000 50 0 0 0 0 0 0 0 1 0 0 0 100 0 0 1000
ctxt 500000
btime 1700000000
processes 10000
procs_running 2
procs_blocked 0
",
        );

        // Disk statistics
        fs.add_file(
            "/proc/diskstats",
            "\
   8       0 sda 12345 100 987654 5000 6789 50 456789 3000 0 4000 8000 0 0 0 0
   8       1 sda1 10000 80 800000 4000 5000 40 400000 2500 0 3500 6500 0 0 0 0
 259       0 nvme0n1 50000 200 2000000 10000 30000 150 1500000 8000 5 15000 18000 0 0 0 0
",
        );

        // Network device statistics
        fs.add_file(
            "/proc/net/dev",
            "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 12345678     9876    0    0    0     0          0         0 12345678     9876    0    0    0     0       0          0
  eth0: 987654321   654321    5   10    0     0          0       100 123456789   456789    2    5    0     0       0          0
",
        );

        // PSI (Pressure Stall Information)
        fs.add_file(
            "/proc/pressure/cpu",
            "some avg10=0.50 avg60=0.30 avg300=0.20 total=1234567\n",
        );
        fs.add_file(
            "/proc/pressure/memory",
            "\
some avg10=0.10 avg60=0.08 avg300=0.05 total=500000
full avg10=0.02 avg60=0.01 avg300=0.01 total=100000
",
        );
        fs.add_file(
            "/proc/pressure/io",
            "\
some avg10=1.50 avg60=1.00 avg300=0.80 total=5000000
full avg10=0.50 avg60=0.30 avg300=0.20 total=1000000
",
        );

        // Virtual memory statistics
        fs.add_file(
            "/proc/vmstat",
            "\
pgpgin 123456
pgpgout 654321
pswpin 100
pswpout 200
pgfault 999999
pgmajfault 1234
pgsteal_kswapd 5000
pgsteal_direct 1000
pgscan_kswapd 10000
pgscan_direct 2000
oom_kill 0
",
        );

        // Network SNMP statistics
        fs.add_file(
            "/proc/net/snmp",
            "\
Ip: Forwarding DefaultTTL InReceives InHdrErrors InAddrErrors ForwDatagrams InUnknownProtos InDiscards InDelivers OutRequests OutDiscards OutNoRoutes ReasmTimeout ReasmReqds ReasmOKs ReasmFails FragOKs FragFails FragCreates
Ip: 1 64 1000000 0 0 0 0 0 999900 800000 0 0 0 0 0 0 0 0 0
Tcp: RtoAlgorithm RtoMin RtoMax MaxConn ActiveOpens PassiveOpens AttemptFails EstabResets CurrEstab InSegs OutSegs RetransSegs InErrs OutRsts InCsumErrors
Tcp: 1 200 120000 -1 5000 3000 100 50 150 500000 450000 1000 10 200 0
Udp: InDatagrams NoPorts InErrors OutDatagrams RcvbufErrors SndbufErrors InCsumErrors IgnoredMulti MemErrors
Udp: 100000 500 5 80000 0 0 0 0 0
",
        );

        // Network extended statistics (TcpExt)
        fs.add_file(
            "/proc/net/netstat",
            "\
TcpExt: SyncookiesSent SyncookiesRecv SyncookiesFailed EmbryonicRsts PruneCalled RcvPruned OfoPruned OutOfWindowIcmps LockDroppedIcmps ArpFilter TW TWRecycled TWKilled PAWSActive PAWSEstab DelayedACKs DelayedACKLocked DelayedACKLost ListenOverflows ListenDrops TCPHPHits TCPPureAcks TCPHPAcks TCPRenoRecovery TCPSackRecovery TCPSACKReneging TCPSACKReorder TCPRenoReorder TCPTSReorder TCPFullUndo TCPPartialUndo TCPDSACKUndo TCPLossUndo TCPLostRetransmit TCPRenoFailures TCPSackFailures TCPLossFailures TCPFastRetrans TCPSlowStartRetrans TCPTimeouts TCPLossProbes TCPLossProbeRecovery TCPRenoRecoveryFail TCPSackRecoveryFail TCPRcvCollapsed TCPBacklogCoalesce TCPDSACKOldSent TCPDSACKOfoSent TCPDSACKRecv TCPDSACKOfoRecv TCPAbortOnData TCPAbortOnClose TCPAbortOnMemory TCPAbortOnTimeout TCPAbortOnLinger TCPAbortFailed TCPMemoryPressures TCPMemoryPressuresChrono TCPSACKDiscard TCPDSACKIgnoredOld TCPDSACKIgnoredNoUndo TCPSpuriousRTOs TCPMD5NotFound TCPMD5Unexpected TCPMD5Failure TCPSackShifted TCPSackMerged TCPSackShiftFallback TCPBacklogDrop TCPMinTTLDrop TCPDeferAcceptDrop IPReversePathFilter TCPTimeWaitOverflow TCPReqQFullDoCookies TCPReqQFullDrop TCPRetransFail TCPRcvCoalesce TCPOFOQueue TCPOFODrop TCPOFOMerge TCPChallengeACK TCPSYNChallenge TCPFastOpenActive TCPFastOpenActiveFail TCPFastOpenPassive TCPFastOpenPassiveFail TCPFastOpenListenOverflow TCPFastOpenCookieReqd TCPFastOpenBlackhole TCPSpuriousRtxHostQueues TCPAutoCorking TCPFromZeroWindowAdv TCPToZeroWindowAdv TCPWantZeroWindowAdv TCPSynRetrans TCPOrigDataSent TCPHystartTrainDetect TCPHystartTrainCwnd TCPHystartDelayDetect TCPHystartDelayCwnd TCPACKSkippedSynRecv TCPACKSkippedPAWS TCPACKSkippedSeq TCPACKSkippedFinWait2 TCPACKSkippedTimeWait TCPACKSkippedChallenge TCPWinProbe TCPKeepAlive TCPMTUPFail TCPMTUPSuccess TCPDelivered TCPDeliveredCE TCPAckCompressed TCPZeroWindowDrop TCPRcvQDrop TCPWqueueTooBig TCPFastOpenPassiveAltKey TcpTimeoutRehash TcpDuplicateDataRehash TCPDSACKRecvSegs TCPDSACKIgnoredDubious TCPMigrateReqSuccess TCPMigrateReqFailure TCPPLBRehash
TcpExt: 0 0 0 0 0 0 0 0 0 0 1000 0 0 0 0 10000 100 500 25 30 100000 50000 40000 0 100 0 10 5 0 0 0 0 50 0 0 5 0 500 200 150 100 50 0 10 0 5000 1000 50 500 25 100 50 0 10 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 10000 2000 0 100 5 0 0 0 0 0 0 0 0 5000 0 0 0 300 400000 100 1000 50 500 0 0 0 0 0 0 0 1000 0 0 450000 0 1000 0 0 0 0 50 10 600 25 0 0 0
IpExt: InNoRoutes InTruncatedPkts InMcastPkts OutMcastPkts InBcastPkts OutBcastPkts InOctets OutOctets InMcastOctets OutMcastOctets InBcastOctets OutBcastOctets InCsumErrors InNoECTPkts InECT1Pkts InECT0Pkts InCEPkts ReasmOverlaps
IpExt: 0 0 1000 500 5000 100 10000000000 5000000000 100000 50000 500000 10000 0 1000000 0 0 0 0
",
        );

        // PID 1 - init/systemd
        fs.add_process(
            1,
            "1 (systemd) S 0 1 1 0 -1 4194560 50000 1000000 100 500 1000 500 2000 1000 20 0 1 0 1 170000000 3000 18446744073709551615 0 0 0 0 0 0 0 0 1073745152 0 0 0 17 0 0 0 0 0 0 0 0 0 0 0 0 0 0",
            "\
Name:\tsystemd
Pid:\t1
PPid:\t0
Uid:\t0\t0\t0\t0
Gid:\t0\t0\t0\t0
VmPeak:\t  200000 kB
VmSize:\t  170000 kB
VmRSS:\t    12000 kB
VmData:\t   10000 kB
VmStk:\t      136 kB
VmLib:\t    10000 kB
VmSwap:\t        0 kB
VmLck:\t        0 kB
voluntary_ctxt_switches:\t1000
nonvoluntary_ctxt_switches:\t100
",
            "rchar: 100000000\nwchar: 50000000\nsyscr: 50000\nsyscw: 25000\nread_bytes: 10000000\nwrite_bytes: 5000000\ncancelled_write_bytes: 0\n",
            "/sbin/init\0",
            "systemd\n",
        );

        // PID 1000 - bash shell
        fs.add_process(
            1000,
            "1000 (bash) S 999 1000 1000 34816 1001 4194304 5000 50000 0 0 100 50 200 100 20 0 1 0 100000 25000000 2000 18446744073709551615 0 0 0 0 0 0 65536 3670020 1266777851 0 0 0 17 2 0 0 0 0 0 0 0 0 0 0 0 0 0",
            "\
Name:\tbash
Pid:\t1000
PPid:\t999
Uid:\t1000\t1000\t1000\t1000
Gid:\t1000\t1000\t1000\t1000
VmPeak:\t   30000 kB
VmSize:\t   25000 kB
VmRSS:\t    8000 kB
VmData:\t    2000 kB
VmStk:\t      136 kB
VmLib:\t    3000 kB
VmSwap:\t        0 kB
VmLck:\t        0 kB
voluntary_ctxt_switches:\t500
nonvoluntary_ctxt_switches:\t50
",
            "rchar: 1000000\nwchar: 500000\nsyscr: 5000\nsyscw: 2500\nread_bytes: 100000\nwrite_bytes: 50000\ncancelled_write_bytes: 0\n",
            "/bin/bash\0--login\0",
            "bash\n",
        );

        // PID 1001 - cat command (child of bash)
        fs.add_process(
            1001,
            "1001 (cat) R 1000 1000 1000 34816 1001 4194304 100 0 0 0 5 2 0 0 20 0 1 0 100100 5000000 500 18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 17 1 0 0 0 0 0 0 0 0 0 0 0 0 0",
            "\
Name:\tcat
Pid:\t1001
PPid:\t1000
Uid:\t1000\t1000\t1000\t1000
Gid:\t1000\t1000\t1000\t1000
VmPeak:\t    6000 kB
VmSize:\t    5000 kB
VmRSS:\t    2000 kB
VmData:\t     200 kB
VmStk:\t      136 kB
VmLib:\t    2000 kB
VmSwap:\t        0 kB
VmLck:\t        0 kB
voluntary_ctxt_switches:\t10
nonvoluntary_ctxt_switches:\t2
",
            "rchar: 10000\nwchar: 10000\nsyscr: 100\nsyscw: 100\nread_bytes: 4096\nwrite_bytes: 4096\ncancelled_write_bytes: 0\n",
            "/bin/cat\0file.txt\0",
            "cat\n",
        );

        fs
    }

    /// Creates a system under high CPU load.
    ///
    /// Multiple processes consuming significant CPU time.
    pub fn high_cpu_load() -> Self {
        let mut fs = Self::typical_system();

        // Modify /proc/stat to show high CPU usage
        fs.add_file(
            "/proc/stat",
            "\
cpu  80000 1000 15000 5000 500 1000 500 0 0 0
cpu0 20000 250 3750 1250 125 250 125 0 0 0
cpu1 20000 250 3750 1250 125 250 125 0 0 0
cpu2 20000 250 3750 1250 125 250 125 0 0 0
cpu3 20000 250 3750 1250 125 250 125 0 0 0
intr 5000000 50 0 0 0 0 0 0 0 1 0 0 0 100 0 0 5000
ctxt 2000000
btime 1700000000
processes 50000
procs_running 8
procs_blocked 2
",
        );

        fs.add_file("/proc/loadavg", "4.50 3.20 2.10 8/200 5000\n");

        // Add CPU-intensive process
        fs.add_process(
            2000,
            "2000 (stress) R 1 2000 2000 0 -1 4194304 1000 0 0 0 500000 10000 0 0 20 0 4 0 200000 100000000 10000 18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0 0 0 0 0 0 0 0 0",
            "\
Name:\tstress
Pid:\t2000
PPid:\t1
Uid:\t0\t0\t0\t0
Gid:\t0\t0\t0\t0
VmPeak:\t  100000 kB
VmSize:\t  100000 kB
VmRSS:\t   40000 kB
VmData:\t   50000 kB
VmStk:\t      136 kB
VmLib:\t    3000 kB
VmSwap:\t        0 kB
VmLck:\t        0 kB
voluntary_ctxt_switches:\t100
nonvoluntary_ctxt_switches:\t50000
",
            "rchar: 0\nwchar: 0\nsyscr: 0\nsyscw: 0\nread_bytes: 0\nwrite_bytes: 0\ncancelled_write_bytes: 0\n",
            "/usr/bin/stress\0--cpu\x004\0",
            "stress\n",
        );

        fs
    }

    /// Creates a system with memory pressure (low free memory, swap in use).
    pub fn memory_pressure() -> Self {
        let mut fs = Self::typical_system();

        fs.add_file(
            "/proc/meminfo",
            "\
MemTotal:       16384000 kB
MemFree:          256000 kB
MemAvailable:     512000 kB
Buffers:           64000 kB
Cached:           256000 kB
SwapCached:       128000 kB
Active:         12000000 kB
Inactive:        3000000 kB
SwapTotal:       4096000 kB
SwapFree:        1024000 kB
Dirty:            102400 kB
Writeback:         10240 kB
Slab:             800000 kB
SReclaimable:     200000 kB
",
        );

        // Add memory-hungry process
        fs.add_process(
            3000,
            "3000 (memhog) S 1 3000 3000 0 -1 4194304 5000000 0 10000 0 1000 500 0 0 20 0 1 0 300000 14000000000 3500000 18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0 0 0 0 0 0 0 0 0",
            "\
Name:\tmemhog
Pid:\t3000
PPid:\t1
Uid:\t1000\t1000\t1000\t1000
Gid:\t1000\t1000\t1000\t1000
VmPeak:\t14000000 kB
VmSize:\t14000000 kB
VmRSS:\t12000000 kB
VmData:\t13500000 kB
VmStk:\t      136 kB
VmLib:\t    3000 kB
VmSwap:\t 2000000 kB
VmLck:\t        0 kB
voluntary_ctxt_switches:\t10000
nonvoluntary_ctxt_switches:\t5000
",
            "rchar: 50000000000\nwchar: 1000000\nsyscr: 10000000\nsyscw: 1000\nread_bytes: 40000000000\nwrite_bytes: 0\ncancelled_write_bytes: 0\n",
            "/usr/local/bin/memhog\0--size\x0014G\0",
            "memhog\n",
        );

        fs
    }

    /// Creates a system with a zombie process.
    pub fn with_zombie_process() -> Self {
        let mut fs = Self::typical_system();

        // Add zombie process (state 'Z')
        fs.add_process(
            4000,
            "4000 (defunct) Z 1000 4000 1000 0 -1 4194308 0 0 0 0 0 0 0 0 20 0 1 0 400000 0 0 18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 -1 0 0 0 0 0 0 0 0 0 0 0 0 0 0",
            "\
Name:\tdefunct
Pid:\t4000
PPid:\t1000
Uid:\t1000\t1000\t1000\t1000
Gid:\t1000\t1000\t1000\t1000
VmPeak:\t        0 kB
VmSize:\t        0 kB
VmRSS:\t        0 kB
",
            "", // No io file for zombie
            "",
            "defunct\n",
        );

        fs
    }

    /// Creates a system with processes that have special characters in names.
    pub fn with_special_names() -> Self {
        let mut fs = Self::typical_system();

        // Process with spaces in name (like Firefox's "Web Content")
        fs.add_process(
            5000,
            "5000 (Web Content) S 4999 5000 4999 0 -1 4194304 100000 0 500 0 5000 1000 0 0 20 0 20 0 500000 2000000000 50000 18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0 0 0 0 0 0 0 0 0",
            "\
Name:\tWeb Content
Pid:\t5000
PPid:\t4999
Uid:\t1000\t1000\t1000\t1000
Gid:\t1000\t1000\t1000\t1000
VmPeak:\t 2000000 kB
VmSize:\t 2000000 kB
VmRSS:\t  200000 kB
VmData:\t 1500000 kB
VmStk:\t      136 kB
VmLib:\t  100000 kB
VmSwap:\t        0 kB
VmLck:\t        0 kB
voluntary_ctxt_switches:\t50000
nonvoluntary_ctxt_switches:\t10000
",
            "rchar: 500000000\nwchar: 100000000\nsyscr: 100000\nsyscw: 50000\nread_bytes: 100000000\nwrite_bytes: 50000000\ncancelled_write_bytes: 1000000\n",
            "/usr/lib/firefox/firefox\0-contentproc\0",
            "Web Content\n",
        );

        // Process with parentheses in name
        fs.add_process(
            5001,
            "5001 (test(1)) S 1 5001 5001 0 -1 4194304 1000 0 0 0 10 5 0 0 20 0 1 0 500100 10000000 1000 18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 17 0 0 0 0 0 0 0 0 0 0 0 0 0 0",
            "\
Name:\ttest(1)
Pid:\t5001
PPid:\t1
Uid:\t1000\t1000\t1000\t1000
Gid:\t1000\t1000\t1000\t1000
VmPeak:\t   10000 kB
VmSize:\t   10000 kB
VmRSS:\t    4000 kB
VmData:\t    2000 kB
VmStk:\t      136 kB
VmLib:\t    2000 kB
VmSwap:\t        0 kB
VmLck:\t        0 kB
voluntary_ctxt_switches:\t100
nonvoluntary_ctxt_switches:\t10
",
            "rchar: 10000\nwchar: 5000\nsyscr: 100\nsyscw: 50\nread_bytes: 4096\nwrite_bytes: 2048\ncancelled_write_bytes: 0\n",
            "/usr/bin/test(1)\0",
            "test(1)\n",
        );

        fs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::traits::FileSystem;
    use std::path::Path;

    #[test]
    fn test_typical_system_has_required_files() {
        let fs = MockFs::typical_system();

        // System files
        assert!(fs.exists(Path::new("/proc/meminfo")));
        assert!(fs.exists(Path::new("/proc/stat")));
        assert!(fs.exists(Path::new("/proc/loadavg")));
        assert!(fs.exists(Path::new("/proc/uptime")));

        // Processes
        assert!(fs.exists(Path::new("/proc/1")));
        assert!(fs.exists(Path::new("/proc/1000")));
        assert!(fs.exists(Path::new("/proc/1001")));
    }

    #[test]
    fn test_high_cpu_load_has_stress_process() {
        let fs = MockFs::high_cpu_load();
        assert!(fs.exists(Path::new("/proc/2000")));

        let loadavg = fs.read_to_string(Path::new("/proc/loadavg")).unwrap();
        assert!(loadavg.starts_with("4.50")); // High load
    }

    #[test]
    fn test_memory_pressure_shows_low_free_memory() {
        let fs = MockFs::memory_pressure();
        let meminfo = fs.read_to_string(Path::new("/proc/meminfo")).unwrap();
        assert!(meminfo.contains("MemFree:          256000 kB"));
    }

    #[test]
    fn test_zombie_process() {
        let fs = MockFs::with_zombie_process();
        let stat = fs.read_to_string(Path::new("/proc/4000/stat")).unwrap();
        assert!(stat.contains(") Z ")); // Zombie state
    }

    #[test]
    fn test_special_names() {
        let fs = MockFs::with_special_names();

        // Process with spaces
        let stat = fs.read_to_string(Path::new("/proc/5000/stat")).unwrap();
        assert!(stat.contains("(Web Content)"));

        // Process with parentheses
        let stat = fs.read_to_string(Path::new("/proc/5001/stat")).unwrap();
        assert!(stat.contains("(test(1))"));
    }
}
