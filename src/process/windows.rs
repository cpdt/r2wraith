use std::ffi::CStr;
use windows::Win32::Foundation::{CloseHandle, HANDLE, MAX_PATH, PSTR, WAIT_TIMEOUT};
use windows::Win32::System::ProcessStatus::{K32EnumProcessModules, K32GetModuleBaseNameA};
use windows::Win32::System::Threading::{HIGH_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_SET_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, PROCESS_VM_READ, REALTIME_PRIORITY_CLASS, SetPriorityClass, TerminateProcess, WaitForSingleObject};
use crate::config::Priority;

pub enum StopProcessError {
    TerminateFailed,
    TimedOut,
}

pub struct Process {
    pub id: u32,
    pub name: String,

    handle: HANDLE,
}

impl Process {
    pub fn new(pid: u32) -> Option<Self> {
        if pid == 0 {
            return None;
        }

        let h_process = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_SET_INFORMATION | PROCESS_VM_READ | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid) };
        if h_process.is_invalid() {
            return None;
        }

        let mut h_mod = 0;
        let mut cb_needed = 0;
        if !unsafe { K32EnumProcessModules(h_process, &mut h_mod, std::mem::size_of_val(&h_mod) as u32, &mut cb_needed) }.as_bool() {
            unsafe { CloseHandle(h_process) };
            return None;
        }

        let mut process_name: [u8; MAX_PATH as usize] = [0; MAX_PATH as usize];
        unsafe {
            K32GetModuleBaseNameA(h_process, h_mod, PSTR(&mut process_name[0] as *mut _), MAX_PATH);
        }

        let name = unsafe { CStr::from_ptr(&process_name[0] as *const u8 as *const _) }.to_string_lossy().into_owned();
        Some(Process {
            id: pid,
            name,
            handle: h_process,
        })
    }

    pub fn set_priority(&self, priority: Priority) -> Result<(), ()> {
        let priority_class = match priority {
            Priority::Normal => NORMAL_PRIORITY_CLASS,
            Priority::High => HIGH_PRIORITY_CLASS,
            Priority::RealTime => REALTIME_PRIORITY_CLASS,
        };
        let could_set = unsafe { SetPriorityClass(self.handle, priority_class) };
        if could_set.as_bool() {
            Ok(())
        } else {
            Err(())
        }
    }

    pub fn is_running(&self) -> bool {
        let ret = unsafe { WaitForSingleObject(self.handle, 0) };
        ret == WAIT_TIMEOUT
    }

    pub fn stop(&self) -> Result<(), StopProcessError> {
        let could_terminate = unsafe { TerminateProcess(self.handle, 0) };
        if could_terminate.as_bool() {
            // Wait for the process to terminate
            let ret = unsafe { WaitForSingleObject(self.handle, 10000) };
            if ret == WAIT_TIMEOUT {
                Err(StopProcessError::TimedOut)
            } else {
                Ok(())
            }
        } else {
            Err(StopProcessError::TerminateFailed)
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}
