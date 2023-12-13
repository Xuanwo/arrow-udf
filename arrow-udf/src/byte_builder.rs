// Copyright 2023 RisingWave Labs
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Specialized byte builder that supports partial writes.

use arrow_array::{
    array::GenericByteArray,
    types::{ByteArrayType, GenericBinaryType, GenericStringType},
};
use arrow_buffer::{ArrowNativeType, BufferBuilder, NullBufferBuilder};
use arrow_data::ArrayDataBuilder;

pub type StringBuilder = GenericByteBuilder<GenericStringType<i32>>;
pub type BinaryBuilder = GenericByteBuilder<GenericBinaryType<i32>>;

/// A specialized byte builder that supports partial writes.
pub struct GenericByteBuilder<T: ByteArrayType> {
    value_builder: Vec<u8>,
    offsets_builder: BufferBuilder<T::Offset>,
    null_buffer_builder: NullBufferBuilder,
}

impl<T: ByteArrayType> GenericByteBuilder<T> {
    /// Creates a new [`GenericByteBuilder`].
    pub fn new() -> Self {
        Self::with_capacity(1024, 1024)
    }

    /// Creates a new [`GenericByteBuilder`].
    ///
    /// - `item_capacity` is the number of items to pre-allocate.
    ///   The size of the preallocated buffer of offsets is the number of items plus one.
    /// - `data_capacity` is the total number of bytes of data to pre-allocate
    ///   (for all items, not per item).
    pub fn with_capacity(item_capacity: usize, data_capacity: usize) -> Self {
        let mut offsets_builder = BufferBuilder::<T::Offset>::new(item_capacity + 1);
        offsets_builder.append(T::Offset::from_usize(0).unwrap());
        Self {
            value_builder: Vec::with_capacity(data_capacity),
            offsets_builder,
            null_buffer_builder: NullBufferBuilder::new(item_capacity),
        }
    }

    #[inline]
    fn next_offset(&self) -> T::Offset {
        T::Offset::from_usize(self.value_builder.len()).expect("byte array offset overflow")
    }

    /// Appends a value into the builder.
    ///
    /// # Panics
    ///
    /// Panics if the resulting length of [`Self::values_slice`] would exceed `T::Offset::MAX`
    #[inline]
    pub fn append_value(&mut self, value: impl AsRef<T::Native>) {
        self.value_builder
            .extend_from_slice(value.as_ref().as_ref());
        self.null_buffer_builder.append(true);
        self.offsets_builder.append(self.next_offset());
    }

    /// Append a null value into the builder.
    #[inline]
    pub fn append_null(&mut self) {
        self.null_buffer_builder.append(false);
        self.offsets_builder.append(self.next_offset());
    }

    /// Returns a writer that can be used to write bytes.
    pub fn writer(&mut self) -> ByteWriter<'_, T> {
        ByteWriter {
            begin_offset: self.value_builder.len(),
            builder: self,
        }
    }

    /// Returns the number of binary slots in the builder
    fn len(&self) -> usize {
        self.null_buffer_builder.len()
    }

    /// Builds the [`GenericByteArray`] and reset this builder.
    pub fn finish(&mut self) -> GenericByteArray<T> {
        let array_type = T::DATA_TYPE;
        let array_builder = ArrayDataBuilder::new(array_type)
            .len(self.len())
            .add_buffer(self.offsets_builder.finish())
            .add_buffer(std::mem::take(&mut self.value_builder).into())
            .nulls(self.null_buffer_builder.finish());

        self.offsets_builder.append(self.next_offset());
        let array_data = unsafe { array_builder.build_unchecked() };
        GenericByteArray::from(array_data)
    }
}

pub struct ByteWriter<'a, T: ByteArrayType> {
    builder: &'a mut GenericByteBuilder<T>,
    begin_offset: usize,
}

impl<T: ByteArrayType> std::io::Write for ByteWriter<'_, T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.builder.value_builder.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<T: ByteArrayType> std::fmt::Write for ByteWriter<'_, T> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.builder.value_builder.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

impl<T: ByteArrayType> ByteWriter<'_, T> {
    pub fn finish(self) {
        self.builder.null_buffer_builder.append(true);
        self.builder
            .offsets_builder
            .append(self.builder.next_offset());
        std::mem::forget(self)
    }
}

impl<T: ByteArrayType> Drop for ByteWriter<'_, T> {
    fn drop(&mut self) {
        self.builder.value_builder.truncate(self.begin_offset);
    }
}
