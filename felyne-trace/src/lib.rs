#![deny(missing_docs)]
#![deny(broken_intra_doc_links)]
//! # Felyne-trace
//!
//! This module offers tools to read, write, and parse anonymised voice data traces from Felyne.
//!
//! ## Format
//! Trace files contain a single [`FelyneTrace`], encoded with bincode and compressed using zlib.
//! Struct-level documentation should explain the inner format, structure, and purpose of fields.
//!
//! [`FelyneTrace`]: crate::FelyneTrace

mod consts;
mod event;
mod extension;
mod label;
pub mod traces;

pub use self::{consts::*, event::*, extension::*, label::*, traces::FelyneTrace};

#[cfg(feature = "async")]
use async_bincode::tokio::{AsyncBincodeReader, AsyncBincodeWriter};
#[cfg(feature = "async")]
use async_compression::{
	tokio::bufread::ZlibDecoder as AsyncZlibDecoder,
	tokio::write::ZlibEncoder as AsyncZlibEncoder,
	Level,
};
use std::io::{Error as IoError, ErrorKind, Read, Result as IoResult, Write};
#[cfg(feature = "async")]
use tokio::io::{AsyncBufRead, AsyncWrite, AsyncWriteExt};

/// Reads a compressed, bincoded [`FelyneTrace`] synchronously.
///
/// [`FelyneTrace`]: crate::FelyneTrace
pub fn read<R: Read>(reader: R) -> IoResult<FelyneTrace> {
	let mut reader_shell = flate2::read::ZlibDecoder::new(reader);
	bincode::deserialize_from(&mut reader_shell)
		.map_err(|e| IoError::new(ErrorKind::InvalidData, e))
}

/// Writes a [`FelyneTrace`] in its at-rest format synchronously.
///
/// [`FelyneTrace`]: crate::FelyneTrace
pub fn write<W: Write>(writer: W, trace: &FelyneTrace) -> IoResult<W> {
	let mut writer_shell = flate2::write::ZlibEncoder::new(writer, flate2::Compression::best());
	bincode::serialize_into(&mut writer_shell, trace)
		.map_err(|e| IoError::new(ErrorKind::InvalidData, e))?;
	writer_shell.finish()
}

#[cfg(feature = "async")]
/// Reads a compressed, bincoded [`FelyneTrace`] asynchronously.
///
/// [`FelyneTrace`]: crate::FelyneTrace
pub async fn read_async<R: AsyncBufRead + Unpin>(reader: R) -> IoResult<FelyneTrace> {
	let read_shell = AsyncZlibDecoder::new(reader);
	let mut bin_shell = AsyncBincodeReader::from(read_shell);

	futures::StreamExt::next(&mut bin_shell)
		.await
		.transpose()
		.map_err(|e| IoError::new(ErrorKind::InvalidData, e))
		.and_then(|opt| {
			opt.ok_or_else(|| IoError::new(ErrorKind::InvalidData, "No decodable trace found!"))
		})
}

#[cfg(feature = "async")]
/// Writes a [`FelyneTrace`] in its at-rest format asynchronously.
///
/// [`FelyneTrace`]: crate::FelyneTrace
pub async fn write_async<W: AsyncWrite + Unpin>(mut writer: W, trace: &FelyneTrace) -> IoResult<W> {
	let mut write_shell = AsyncZlibEncoder::with_quality(&mut writer, Level::Best);
	let mut bin_shell = AsyncBincodeWriter::from(&mut write_shell).for_async();

	futures::SinkExt::send(&mut bin_shell, trace)
		.await
		.map_err(|e| IoError::new(ErrorKind::InvalidData, e))?;

	write_shell.shutdown().await.map(|_| writer)
}
