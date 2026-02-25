use std::path::PathBuf;

/// Default timeout in seconds (10 minutes)
pub const DEFAULT_TIMEOUT_SECS: u64 = 600;

/// Maximum allowed timeout in seconds (1 hour)
pub const MAX_TIMEOUT_SECS: u64 = 3600;

/// Minimum allowed timeout in seconds
pub const MIN_TIMEOUT_SECS: u64 = 1;

/// Find a binary by name, checking an environment variable override first.
pub fn find_binary(name: &str, env_override: &str) -> Option<PathBuf> {
    if let Ok(path) = std::env::var(env_override) {
        let p = PathBuf::from(&path);
        if p.exists() {
            return Some(p);
        }
    }
    which::which(name).ok()
}

/// Windows Job Object: assigns a child process to a job configured with
/// KILL_ON_JOB_CLOSE so that the entire process tree (including grandchildren
/// spawned by cmd.exe) is terminated when the job handle is closed.
#[cfg(windows)]
#[allow(clippy::upper_case_acronyms)]
pub mod job_object {
    use std::ffi::c_void;

    type HANDLE = *mut c_void;
    type BOOL = i32;

    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: i32 = 9;
    const PROCESS_SET_QUOTA: u32 = 0x0100;
    const PROCESS_TERMINATE: u32 = 0x0001;

    #[repr(C)]
    struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    struct IO_COUNTERS {
        read_operations_count: u64,
        write_operations_count: u64,
        other_operations_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        basic: JOBOBJECT_BASIC_LIMIT_INFORMATION,
        io_info: IO_COUNTERS,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    extern "system" {
        fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> HANDLE;
        fn OpenProcess(desired_access: u32, inherit_handle: BOOL, pid: u32) -> HANDLE;
        fn AssignProcessToJobObject(job: HANDLE, process: HANDLE) -> BOOL;
        fn SetInformationJobObject(job: HANDLE, class: i32, info: *const c_void, len: u32) -> BOOL;
        fn TerminateJobObject(job: HANDLE, exit_code: u32) -> BOOL;
        fn CloseHandle(handle: HANDLE) -> BOOL;
    }

    /// RAII wrapper for a Windows Job Object. Terminates the entire process
    /// tree when `terminate()` is called or the handle is dropped.
    pub struct ProcessJob {
        handle: HANDLE,
    }

    // Job handle is an opaque kernel handle, safe to send across threads.
    unsafe impl Send for ProcessJob {}
    unsafe impl Sync for ProcessJob {}

    impl ProcessJob {
        /// Create a job and assign the child (by PID) to it.
        /// Returns None if any Win32 call fails (non-fatal: caller falls
        /// back to child.kill()).
        pub fn assign(pid: u32) -> Option<Self> {
            unsafe {
                let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
                if job.is_null() {
                    return None;
                }

                // Configure: kill all processes when job handle closes
                let info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
                    basic: JOBOBJECT_BASIC_LIMIT_INFORMATION {
                        per_process_user_time_limit: 0,
                        per_job_user_time_limit: 0,
                        limit_flags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                        minimum_working_set_size: 0,
                        maximum_working_set_size: 0,
                        active_process_limit: 0,
                        affinity: 0,
                        priority_class: 0,
                        scheduling_class: 0,
                    },
                    io_info: IO_COUNTERS {
                        read_operations_count: 0,
                        write_operations_count: 0,
                        other_operations_count: 0,
                        read_transfer_count: 0,
                        write_transfer_count: 0,
                        other_transfer_count: 0,
                    },
                    process_memory_limit: 0,
                    job_memory_limit: 0,
                    peak_process_memory_used: 0,
                    peak_job_memory_used: 0,
                };

                if SetInformationJobObject(
                    job,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                    &info as *const _ as *const c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                ) == 0
                {
                    CloseHandle(job);
                    return None;
                }

                // Open the child process and assign it to the job
                let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
                if process.is_null() {
                    CloseHandle(job);
                    return None;
                }

                let ok = AssignProcessToJobObject(job, process);
                CloseHandle(process);
                if ok == 0 {
                    CloseHandle(job);
                    return None;
                }

                Some(ProcessJob { handle: job })
            }
        }

        /// Terminate every process in the job immediately.
        pub fn terminate(&self) {
            unsafe {
                TerminateJobObject(self.handle, 1);
            }
        }
    }

    impl Drop for ProcessJob {
        fn drop(&mut self) {
            unsafe {
                // KILL_ON_JOB_CLOSE ensures all remaining processes die
                // when the last handle is closed.
                CloseHandle(self.handle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_constants() {
        assert_eq!(DEFAULT_TIMEOUT_SECS, 600);
        assert_eq!(MAX_TIMEOUT_SECS, 3600);
        assert_eq!(MIN_TIMEOUT_SECS, 1);
        assert!(MIN_TIMEOUT_SECS < DEFAULT_TIMEOUT_SECS);
        assert!(DEFAULT_TIMEOUT_SECS < MAX_TIMEOUT_SECS);
    }

    #[test]
    fn test_find_binary_nonexistent() {
        assert!(find_binary("this_binary_does_not_exist_xyz", "NONEXISTENT_ENV_VAR").is_none());
    }
}
