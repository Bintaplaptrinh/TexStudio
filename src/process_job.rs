//! Keep MiKTeX and its descendants within the Windows application lifetime.
use anyhow::{Context, Result};
use std::{
    mem::{size_of, zeroed},
    os::windows::io::AsRawHandle,
    process::Child,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::*,
};

pub struct ProcessJob(HANDLE);
impl ProcessJob {
    pub fn attach(child: &mut Child) -> Result<Self> {
        // SAFETY: null attributes/name request an unnamed, non-inheritable job.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::last_os_error())
                .context("Could not create compiler process group");
        }
        let guard = Self(job);
        // SAFETY: this Windows data structure permits zero initialization.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: handles are live; info has the requested layout and size.
        let ok = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) != 0
                && AssignProcessToJobObject(job, child.as_raw_handle()) != 0
        };
        if !ok {
            let error = std::io::Error::last_os_error();
            let _ = child.kill();
            let _ = child.wait();
            return Err(error).context("Could not supervise MiKTeX processes");
        }
        Ok(guard)
    }
}
impl Drop for ProcessJob {
    fn drop(&mut self) {
        // SAFETY: this guard uniquely owns the job handle.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
