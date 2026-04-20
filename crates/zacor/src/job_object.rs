use crate::error::*;
use std::os::windows::io::AsRawHandle;
use std::process::Child;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::JobObjects::*;

pub struct JobObject {
    handle: isize,
}

impl JobObject {
    pub fn setup() -> Result<Self> {
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle == 0 {
                bail!("failed to create Job Object");
            }

            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

            let ret = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );

            if ret == 0 {
                CloseHandle(handle);
                bail!("failed to configure Job Object");
            }

            Ok(JobObject { handle })
        }
    }

    pub fn assign(&self, child: &Child) -> Result<()> {
        unsafe {
            let ret = AssignProcessToJobObject(self.handle, child.as_raw_handle() as isize);
            if ret == 0 {
                bail!("failed to assign process to Job Object");
            }
            Ok(())
        }
    }
}

impl Drop for JobObject {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_object_creation() {
        let job = JobObject::setup();
        assert!(job.is_ok(), "Job Object creation should succeed");
    }

    #[test]
    fn test_job_object_assign() {
        let job = JobObject::setup().unwrap();
        let mut child = std::process::Command::new("cmd")
            .args(["/C", "echo hello"])
            .spawn()
            .unwrap();
        let result = job.assign(&child);
        assert!(result.is_ok(), "Job Object assignment should succeed");
        child.wait().unwrap();
    }

    #[test]
    fn test_job_object_fallback_behavior() {
        // Verify that if setup succeeds, it returns Ok
        // If it were to fail (can't easily simulate), code should handle it gracefully
        let result = JobObject::setup();
        assert!(result.is_ok());
        drop(result); // Ensure Drop runs without panic
    }
}
