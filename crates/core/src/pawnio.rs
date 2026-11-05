//! Talks to the PawnIO driver and its signed SMBus module.
use crate::bus::{Bus, BusError, StickAddr};
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};
use windows_sys::Win32::System::IO::DeviceIoControl;

static MODULE: &[u8] = include_bytes!("../modules/SmbusPIIX4.bin");

// CTL_CODE(41394, function, METHOD_BUFFERED, FILE_ANY_ACCESS), from pawnio_um.h.
const IOCTL_LOAD_BINARY: u32 = (41394 << 16) | (0x821 << 2);
const IOCTL_EXECUTE: u32 = (41394 << 16) | (0x841 << 2);

const READ: u64 = 1;
const WRITE: u64 = 0;
const PROTOCOL_BYTE_DATA: u64 = 2;
const PROTOCOL_BLOCK_DATA: u64 = 5;
const LOCK_TIMEOUT_MS: u32 = 100;
const ERROR_LOCK_TIMEOUT: i32 = -2;

const GENERIC_READ_WRITE: u32 = 0x8000_0000 | 0x4000_0000;
const FILE_SHARE_ALL: u32 = 1 | 2 | 4;

#[repr(C)]
struct UnicodeString { length: u16, maximum_length: u16, buffer: *const u16 }

#[repr(C)]
struct ObjectAttributes {
    length: u32,
    root_directory: HANDLE,
    object_name: *const UnicodeString,
    attributes: u32,
    security_descriptor: *const c_void,
    security_quality_of_service: *const c_void,
}

#[repr(C)]
struct IoStatusBlock { status: usize, information: usize }

#[link(name = "ntdll")]
extern "system" {
    fn NtOpenFile(handle: *mut HANDLE, access: u32, attributes: *const ObjectAttributes,
        io_status: *mut IoStatusBlock, share: u32, options: u32) -> i32;
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

pub struct PawnIoBus {
    device: HANDLE,
    smbus_lock: HANDLE,
}

impl PawnIoBus {
    pub fn open() -> Result<PawnIoBus, BusError> {
        let path: Vec<u16> = "\\Device\\PawnIO".encode_utf16().collect();
        let name = UnicodeString {
            length: (path.len() * 2) as u16,
            maximum_length: (path.len() * 2) as u16,
            buffer: path.as_ptr(),
        };
        let attributes = ObjectAttributes {
            length: std::mem::size_of::<ObjectAttributes>() as u32,
            root_directory: null_mut(),
            object_name: &name,
            attributes: 0,
            security_descriptor: null(),
            security_quality_of_service: null(),
        };
        let mut io_status = IoStatusBlock { status: 0, information: 0 };
        let mut device: HANDLE = null_mut();
        let status = unsafe {
            NtOpenFile(&mut device, GENERIC_READ_WRITE, &attributes, &mut io_status, FILE_SHARE_ALL, 0)
        };
        if status < 0 {
            return Err(BusError::Os(status));
        }
        let lock_name = wide("Global\\Access_SMBUS.HTP.Method");
        let smbus_lock = unsafe { CreateMutexW(null(), 0, lock_name.as_ptr()) };
        if smbus_lock.is_null() {
            unsafe { CloseHandle(device) };
            return Err(BusError::Os(unsafe { GetLastError() } as i32));
        }
        let mut bus = PawnIoBus { device, smbus_lock };
        bus.load_module()?;
        if bus.current_port()? != 0 {
            return Err(BusError::UnexpectedPort);
        }
        Ok(bus)
    }

    fn load_module(&mut self) -> Result<(), BusError> {
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(self.device, IOCTL_LOAD_BINARY, MODULE.as_ptr().cast(), MODULE.len() as u32,
                null_mut(), 0, &mut returned, null_mut())
        };
        if ok == 0 {
            return Err(BusError::Os(unsafe { GetLastError() } as i32));
        }
        Ok(())
    }

    fn call(&mut self, function: &str, input: &[u64], output: &mut [u64]) -> Result<(), BusError> {
        let mut buffer = vec![0u8; 32 + input.len() * 8];
        buffer[..function.len()].copy_from_slice(function.as_bytes());
        for (slot, value) in buffer[32..].chunks_exact_mut(8).zip(input) {
            slot.copy_from_slice(&value.to_le_bytes());
        }
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(self.device, IOCTL_EXECUTE, buffer.as_ptr().cast(), buffer.len() as u32,
                output.as_mut_ptr().cast(), (output.len() * 8) as u32, &mut returned, null_mut())
        };
        if ok == 0 {
            return Err(BusError::Os(unsafe { GetLastError() } as i32));
        }
        Ok(())
    }

    /// Runs one transfer while holding the lock shared with other hardware tools.
    fn transfer(&mut self, input: &[u64], output: &mut [u64]) -> Result<(), BusError> {
        let wait = unsafe { WaitForSingleObject(self.smbus_lock, LOCK_TIMEOUT_MS) };
        if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
            return Err(BusError::Os(ERROR_LOCK_TIMEOUT));
        }
        let result = self.call("ioctl_smbus_xfer", input, output);
        unsafe { ReleaseMutex(self.smbus_lock) };
        result
    }

    /// Asks for the current port without changing it (-1 means query only).
    fn current_port(&mut self) -> Result<u64, BusError> {
        let mut output = [0u64; 1];
        self.call("ioctl_piix4_port_sel", &[u64::MAX], &mut output)?;
        Ok(output[0])
    }
}

impl Drop for PawnIoBus {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.smbus_lock);
            CloseHandle(self.device);
        }
    }
}

impl Bus for PawnIoBus {
    fn read_byte(&mut self, addr: StickAddr, reg: u8) -> Result<u8, BusError> {
        let mut output = [0u64; 1];
        self.transfer(&[addr.get() as u64, READ, reg as u64, PROTOCOL_BYTE_DATA], &mut output)?;
        Ok(output[0] as u8)
    }

    fn write_byte(&mut self, addr: StickAddr, reg: u8, value: u8) -> Result<(), BusError> {
        self.transfer(&[addr.get() as u64, WRITE, reg as u64, PROTOCOL_BYTE_DATA, value as u64], &mut [])
    }

    fn write_block(&mut self, addr: StickAddr, reg: u8, data: &[u8]) -> Result<(), BusError> {
        if data.is_empty() || data.len() > 32 {
            return Err(BusError::BlockTooLong);
        }
        let mut bytes = [0u8; 40];
        bytes[0] = data.len() as u8;
        bytes[1..=data.len()].copy_from_slice(data);
        let mut input = [addr.get() as u64, WRITE, reg as u64, PROTOCOL_BLOCK_DATA, 0, 0, 0, 0, 0];
        for (slot, chunk) in input[4..].iter_mut().zip(bytes.chunks_exact(8)) {
            *slot = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        self.transfer(&input, &mut [])
    }
}
