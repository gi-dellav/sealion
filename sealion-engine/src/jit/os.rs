use std::{
    ffi::CString,
    ptr::NonNull,
    sync::atomic::{Ordering, fence},
};

use crate::Error;

#[link(name = "m")]
unsafe extern "C" {}

pub(super) trait Memory {
    fn base(&self) -> *mut u8;
    fn len(&self) -> usize;
    unsafe fn publish(&mut self, text_len: usize) -> Result<(), Error>;
}

pub(super) trait Os {
    type Memory: Memory;

    fn page_size() -> usize;
    fn allocate(size: usize) -> Result<Self::Memory, Error>;
    fn lookup(name: &str) -> Option<usize>;
}

pub(super) struct Linux;

pub(super) struct LinuxMemory {
    ptr: NonNull<u8>,
    len: usize,
}

impl Memory for LinuxMemory {
    fn base(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    fn len(&self) -> usize {
        self.len
    }

    unsafe fn publish(&mut self, text_len: usize) -> Result<(), Error> {
        if text_len > 0 {
            unsafe {
                if libc::mprotect(
                    self.base().cast(),
                    text_len,
                    libc::PROT_READ | libc::PROT_EXEC,
                ) != 0
                {
                    return Err(Error::Jit(format!(
                        "failed to make JIT text executable: {}",
                        std::io::Error::last_os_error()
                    )));
                }
            }
        }

        let data_len = self.len.saturating_sub(text_len);
        if data_len > 0 {
            unsafe {
                if libc::mprotect(
                    self.base().add(text_len).cast(),
                    data_len,
                    libc::PROT_READ | libc::PROT_WRITE,
                ) != 0
                {
                    return Err(Error::Jit(format!(
                        "failed to make JIT data writable: {}",
                        std::io::Error::last_os_error()
                    )));
                }
            }
        }

        fence(Ordering::SeqCst);
        Ok(())
    }
}

impl Drop for LinuxMemory {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr.as_ptr().cast(), self.len);
        }
    }
}

impl Os for Linux {
    type Memory = LinuxMemory;

    fn page_size() -> usize {
        unsafe {
            let size = libc::sysconf(libc::_SC_PAGESIZE);
            if size <= 0 { 4096 } else { size as usize }
        }
    }

    fn allocate(size: usize) -> Result<Self::Memory, Error> {
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(Error::Jit(format!(
                "failed to allocate JIT memory: {}",
                std::io::Error::last_os_error()
            )));
        }
        let ptr = NonNull::new(ptr.cast::<u8>())
            .ok_or_else(|| Error::Jit("JIT memory allocation returned null".to_string()))?;
        Ok(LinuxMemory { ptr, len: size })
    }

    fn lookup(name: &str) -> Option<usize> {
        let name = CString::new(name).ok()?;
        let ptr = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
        if ptr.is_null() {
            None
        } else {
            Some(ptr as usize)
        }
    }
}
