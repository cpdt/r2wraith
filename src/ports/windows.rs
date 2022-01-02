use std::fmt::{Display, Formatter};
use std::marker::PhantomData;
use windows::Win32::Foundation::NO_ERROR;
use windows::Win32::Networking::WinSock::ntohs;
use windows::Win32::NetworkManagement::IpHelper::{GetTcpTable, GetUdpTable, MIB_TCPROW_LH, MIB_TCPTABLE, MIB_UDPROW, MIB_UDPTABLE};

#[derive(Debug)]
pub enum PortError {
    GetTableFailed,
}

impl Display for PortError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Could not list open ports")
    }
}

impl std::error::Error for PortError {}

pub struct TcpPortTable {
    inner_table: Box<[u8]>,
}

impl TcpPortTable {
    pub fn new() -> Result<Self, PortError> {
        let mut expected_buffer_size = 0;
        unsafe { GetTcpTable(std::ptr::null_mut(), &mut expected_buffer_size, false) };

        let mut buffer = vec![0u8; expected_buffer_size as usize].into_boxed_slice();
        let result = unsafe { GetTcpTable(&mut buffer[0] as *mut u8 as *mut _, &mut expected_buffer_size, false) };
        if result != NO_ERROR {
            return Err(PortError::GetTableFailed);
        }

        Ok(TcpPortTable {
            inner_table: buffer,
        })
    }

    pub fn iter(&self) -> TcpPortTableIter {
        let table_header = unsafe { &*(&self.inner_table[0] as *const u8 as *const MIB_TCPTABLE) };
        let entry_count = table_header.dwNumEntries as usize;
        let first_entry = &table_header.table[0] as *const MIB_TCPROW_LH;

        unsafe { TcpPortTableIter::new(entry_count, first_entry) }
    }
}

pub struct TcpPortTableIter<'table> {
    remaining_entry_count: usize,
    next_entry: *const MIB_TCPROW_LH,
    table: PhantomData<&'table TcpPortTable>,
}

impl<'table> TcpPortTableIter<'table> {
    unsafe fn new(entry_count: usize, first_entry: *const MIB_TCPROW_LH) -> Self {
        TcpPortTableIter {
            remaining_entry_count: entry_count,
            next_entry: first_entry,
            table: PhantomData,
        }
    }
}

impl<'table> Iterator for TcpPortTableIter<'table> {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_entry_count == 0 {
            return None;
        }

        let next_row = unsafe { &*self.next_entry };
        let port = unsafe { ntohs(next_row.dwLocalPort as u16) };

        self.remaining_entry_count -= 1;
        if self.remaining_entry_count != 0 {
            unsafe { self.next_entry = self.next_entry.add(1) };
        }

        Some(port)
    }
}

pub struct UdpPortTable {
    inner_table: Box<[u8]>,
}

impl UdpPortTable {
    pub fn new() -> Result<Self, PortError> {
        let mut expected_buffer_size = 0;
        unsafe { GetUdpTable(std::ptr::null_mut(), &mut expected_buffer_size, false) };

        let mut buffer = vec![0u8; expected_buffer_size as usize].into_boxed_slice();
        let result = unsafe { GetUdpTable(&mut buffer[0] as *mut u8 as *mut _, &mut expected_buffer_size, false) };
        if result != NO_ERROR {
            return Err(PortError::GetTableFailed);
        }

        Ok(UdpPortTable {
            inner_table: buffer,
        })
    }

    pub fn iter(&self) -> UdpPortTableIter {
        let table_header = unsafe { &*(&self.inner_table[0] as *const u8 as *const MIB_UDPTABLE) };
        let entry_count = table_header.dwNumEntries as usize;
        let first_entry = &table_header.table[0] as *const MIB_UDPROW;

        unsafe { UdpPortTableIter::new(entry_count, first_entry) }
    }
}

pub struct UdpPortTableIter<'table> {
    remaining_entry_count: usize,
    next_entry: *const MIB_UDPROW,
    table: PhantomData<&'table UdpPortTable>,
}

impl<'table> UdpPortTableIter<'table> {
    unsafe fn new(entry_count: usize, first_entry: *const MIB_UDPROW) -> Self {
        UdpPortTableIter {
            remaining_entry_count: entry_count,
            next_entry: first_entry,
            table: PhantomData,
        }
    }
}

impl<'table> Iterator for UdpPortTableIter<'table> {
    type Item = u16;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_entry_count == 0 {
            return None;
        }

        let next_row = unsafe { &*self.next_entry };
        let port = unsafe { ntohs(next_row.dwLocalPort as u16) };

        self.remaining_entry_count -= 1;
        if self.remaining_entry_count != 0 {
            unsafe { self.next_entry = self.next_entry.add(1) };
        }

        Some(port)
    }
}
