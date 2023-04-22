// Copyright (c) 2023 the Hearth contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// This file is part of Hearth.
//
// Hearth is free software: you can redistribute it and/or modify it under the
// terms of the GNU Affero General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Hearth is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more
// details.
//
// You should have received a copy of the GNU Affero General Public License
// along with Hearth. If not, see <https://www.gnu.org/licenses/>.

use anyhow::{anyhow, Context, Result};
use wasmtime::Caller;

pub trait WasmLinker<T: AsMut<Self>> {
    fn add_to_linker(linker: &mut wasmtime::Linker<T>);
}

pub struct GuestMemory<'a> {
    pub bytes: &'a mut [u8],
}

impl<'a> GuestMemory<'a> {
    pub fn from_caller<T>(caller: &mut Caller<'a, T>) -> Result<Self> {
        let memory = caller
            .get_export("memory")
            .ok_or_else(|| anyhow!("Caller does not export memory"))?
            .into_memory()
            .ok_or_else(|| anyhow!("Caller 'memory' export is not a memory"))?;
        let data_ptr = memory.data_ptr(&caller);
        let data_size = memory.data_size(&caller);
        let bytes = unsafe { std::slice::from_raw_parts_mut(data_ptr, data_size) };
        Ok(Self { bytes })
    }

    pub fn get_str(&self, ptr: u32, len: u32) -> Result<&'a mut str> {
        let memory = self.get_slice(ptr as usize, len as usize)?;
        std::str::from_utf8_mut(memory)
            .with_context(|| format!("GuestMemory::get_str({}, {})", ptr, len))
    }

    pub fn get_slice(&self, ptr: usize, len: usize) -> Result<&'a mut [u8]> {
        if ptr + len > self.bytes.len() {
            Err(anyhow!(
                "GuestMemory::get_slice({}, {}) is out-of-bounds",
                ptr,
                len
            ))
        } else {
            unsafe {
                let ptr = self.bytes.as_ptr().add(ptr) as *mut u8;
                Ok(std::slice::from_raw_parts_mut(ptr, len))
            }
        }
    }

    pub fn get_memory_ref<T: bytemuck::Pod>(&self, ptr: u32) -> Result<&'a mut T> {
        let len = std::mem::size_of::<T>();
        let bytes = self.get_slice(ptr as usize, len)?;
        bytemuck::try_from_bytes_mut(bytes).map_err(|err| {
            anyhow!(
                "GuestMemory::get_memory_ref<{}>({}) failed: {:?}",
                std::any::type_name::<T>(),
                ptr,
                err
            )
        })
    }

    pub fn get_memory_slice<T: bytemuck::Pod>(&self, ptr: u32, num: u32) -> Result<&'a mut [T]> {
        let len = num as usize * std::mem::size_of::<T>();
        let bytes = self.get_slice(ptr as usize, len)?;
        bytemuck::try_cast_slice_mut(bytes).map_err(|err| {
            anyhow!(
                "GuestMemory::get_memory_slice<{}>({}, {}) failed: {:?}",
                std::any::type_name::<T>(),
                ptr,
                num,
                err
            )
        })
    }
}
